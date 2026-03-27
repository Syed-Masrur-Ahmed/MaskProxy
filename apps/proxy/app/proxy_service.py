from __future__ import annotations

import ipaddress
import logging
import re
from typing import Any
from urllib.parse import urlparse
from uuid import uuid4

import httpx
from fastapi import HTTPException, Request

from app.config import Settings
from app.detectors import Detector
from app.masking import MappingState, mask_request_payload, rehydrate_value
from app.routing import RouteTarget, Router, extract_routable_text
from app.session_store import SessionStore, SessionStoreError

SESSION_ID_PATTERN = re.compile(r"^[a-zA-Z0-9_-]{1,128}$")
FORWARDED_HEADER_ALLOWLIST = frozenset({"authorization", "content-type", "accept", "user-agent"})
logger = logging.getLogger(__name__)


class UpstreamProxyError(RuntimeError):
    def __init__(self, status_code: int, payload: dict[str, Any]) -> None:
        self.status_code = status_code
        self.payload = payload
        super().__init__(f"upstream request failed with status {status_code}")


class ProxyService:
    def __init__(
        self,
        settings: Settings,
        session_store: SessionStore,
        upstream_transport: httpx.AsyncBaseTransport | None = None,
        local_upstream_transport: httpx.AsyncBaseTransport | None = None,
        detector: Detector | None = None,
        router: Router | None = None,
    ) -> None:
        self._settings = settings
        self._session_store = session_store
        if detector is None:
            raise ValueError("ProxyService requires an explicit detector.")
        if router is None:
            raise ValueError("ProxyService requires an explicit router.")
        self._detector = detector
        self._router = router
        self._cloud_client = httpx.AsyncClient(
            base_url=str(settings.upstream_base_url).rstrip("/"),
            timeout=30.0,
            transport=upstream_transport,
        )
        self._local_client = None
        if settings.local_upstream_base_url:
            if not _looks_like_private_or_local_url(settings.local_upstream_base_url):
                logger.warning(
                    "Local upstream base URL %s does not look local/private; local-routed payloads "
                    "will bypass masking.",
                    settings.local_upstream_base_url,
                )
            self._local_client = httpx.AsyncClient(
                base_url=str(settings.local_upstream_base_url).rstrip("/"),
                timeout=30.0,
                transport=local_upstream_transport,
            )

    async def close(self) -> None:
        await self._cloud_client.aclose()
        if self._local_client is not None:
            await self._local_client.aclose()
        await self._session_store.close()

    async def forward_chat_completion(
        self,
        payload: dict[str, Any],
        request: Request,
    ) -> tuple[dict[str, Any], str | None]:
        session_id = self._resolve_session_id(payload, request)
        route_decision = self._router.route(extract_routable_text(payload))

        if route_decision.target == RouteTarget.LOCAL:
            return await self._forward_local_chat_completion(payload, request)

        # Known v1 limitation: two concurrent requests with the same session ID can race on
        # read-modify-write session updates. We are treating session usage as effectively
        # single-flight for now and will need Redis CAS/Lua scripting in hardening.
        try:
            existing_mapping = await self._session_store.get_mapping(session_id)
        except SessionStoreError as exc:
            raise HTTPException(
                status_code=503,
                detail="PII session state unavailable; refusing to forward masked request.",
            ) from exc

        mapping_state = MappingState.from_placeholder_mapping(existing_mapping)
        masked_payload = mask_request_payload(payload, mapping_state, self._detector)

        try:
            await self._session_store.save_mapping(
                session_id=session_id,
                mapping=mapping_state.placeholder_to_real,
                ttl_seconds=self._settings.mapping_ttl_seconds,
            )
        except SessionStoreError as exc:
            raise HTTPException(
                status_code=503,
                detail="PII session state unavailable; refusing to forward masked request.",
            ) from exc

        try:
            upstream_response = await self._cloud_client.post(
                "/v1/chat/completions",
                json=masked_payload,
                headers=self._build_upstream_headers(request, include_cloud_authorization=True),
            )
            upstream_response.raise_for_status()
        except httpx.HTTPStatusError as exc:
            error_payload = self._parse_upstream_error(exc.response)
            rehydrated_error = rehydrate_value(error_payload, mapping_state.placeholder_to_real)
            raise UpstreamProxyError(exc.response.status_code, rehydrated_error) from exc
        except httpx.HTTPError as exc:
            raise HTTPException(status_code=502, detail="Failed to reach upstream provider.") from exc

        try:
            response_payload = upstream_response.json()
        except ValueError:
            truncated_text = self._truncate_text(upstream_response.text)
            raise UpstreamProxyError(
                502,
                {"error": f"Upstream returned non-JSON success response: {truncated_text}"},
            )
        rehydrated = rehydrate_value(response_payload, mapping_state.placeholder_to_real)
        return rehydrated, session_id

    async def _forward_local_chat_completion(
        self,
        payload: dict[str, Any],
        request: Request,
    ) -> tuple[dict[str, Any], None]:
        if self._local_client is None:
            raise HTTPException(status_code=503, detail="Local route selected but no local upstream is configured.")

        try:
            upstream_response = await self._local_client.post(
                "/v1/chat/completions",
                json=payload,
                headers=self._build_upstream_headers(request, include_cloud_authorization=False),
            )
            upstream_response.raise_for_status()
        except httpx.HTTPStatusError as exc:
            raise UpstreamProxyError(exc.response.status_code, self._parse_upstream_error(exc.response)) from exc
        except httpx.HTTPError as exc:
            raise HTTPException(status_code=502, detail="Failed to reach local upstream provider.") from exc

        try:
            response_payload = upstream_response.json()
        except ValueError:
            truncated_text = self._truncate_text(upstream_response.text)
            raise UpstreamProxyError(
                502,
                {"error": f"Local upstream returned non-JSON success response: {truncated_text}"},
            )

        return response_payload, None

    def _resolve_session_id(self, payload: dict[str, Any], request: Request) -> str:
        header_session_id = request.headers.get("x-session-id")
        if header_session_id is not None:
            return self._validate_session_id(header_session_id)

        if "session_id" in payload:
            body_session_id = payload.get("session_id")
            if body_session_id is None:
                return f"req-{uuid4()}"
            if not isinstance(body_session_id, str):
                raise HTTPException(status_code=400, detail="Invalid session ID.")
            return self._validate_session_id(body_session_id)

        return f"req-{uuid4()}"

    def _build_upstream_headers(
        self,
        request: Request,
        *,
        include_cloud_authorization: bool,
    ) -> dict[str, str]:
        passthrough_headers = {
            key.lower(): value
            for key, value in request.headers.items()
            if key.lower() in FORWARDED_HEADER_ALLOWLIST
        }

        if include_cloud_authorization and self._settings.upstream_api_key:
            passthrough_headers["authorization"] = f"Bearer {self._settings.upstream_api_key}"

        return passthrough_headers

    def _validate_session_id(self, session_id: str) -> str:
        if not SESSION_ID_PATTERN.fullmatch(session_id):
            raise HTTPException(status_code=400, detail="Invalid session ID.")
        return session_id

    def _parse_upstream_error(self, response: httpx.Response) -> dict[str, Any]:
        try:
            payload = response.json()
            if isinstance(payload, dict):
                return payload
            return {"error": payload}
        except ValueError:
            return {"error": self._truncate_text(response.text)}

    def _truncate_text(self, text: str, limit: int = 512) -> str:
        if len(text) <= limit:
            return text
        return f"{text[:limit]}..."


def _looks_like_private_or_local_url(url: str) -> bool:
    parsed = urlparse(url)
    hostname = parsed.hostname
    if hostname is None:
        return False

    if hostname == "localhost" or hostname.endswith(".local"):
        return True

    try:
        address = ipaddress.ip_address(hostname)
    except ValueError:
        return False

    return address.is_loopback or address.is_private
