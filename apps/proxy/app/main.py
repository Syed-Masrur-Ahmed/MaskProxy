from __future__ import annotations

from contextlib import asynccontextmanager
import json
from typing import Any

import httpx
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

from app.config import Settings, get_settings
from app.proxy_service import ProxyService, UpstreamProxyError
from app.session_store import RedisSessionStore, SessionStore


def create_app(
    settings: Settings | None = None,
    session_store: SessionStore | None = None,
    upstream_transport: httpx.AsyncBaseTransport | None = None,
) -> FastAPI:
    resolved_settings = settings or get_settings()
    resolved_store = session_store or RedisSessionStore(resolved_settings.redis_url)
    proxy_service = ProxyService(
        settings=resolved_settings,
        session_store=resolved_store,
        upstream_transport=upstream_transport,
    )

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.proxy_service = proxy_service
        yield
        await proxy_service.close()

    app = FastAPI(title="MaskProxy Proxy", version="0.1.0", lifespan=lifespan)
    app.state.proxy_service = proxy_service
    app.state.settings = resolved_settings

    @app.get("/health")
    async def healthcheck() -> dict[str, str]:
        return {"status": "ok"}

    @app.post("/v1/chat/completions")
    async def chat_completions(request: Request) -> JSONResponse:
        body = await request.body()
        if len(body) > app.state.settings.max_body_bytes:
            return JSONResponse(status_code=413, content={"detail": "Request body too large."})

        try:
            payload: dict[str, Any] = json.loads(body)
        except json.JSONDecodeError:
            return JSONResponse(status_code=400, content={"detail": "Request body must be valid JSON."})

        if not isinstance(payload, dict):
            return JSONResponse(status_code=400, content={"detail": "Request body must be a JSON object."})

        try:
            response_payload, session_id = await app.state.proxy_service.forward_chat_completion(payload, request)
        except UpstreamProxyError as exc:
            return JSONResponse(content=exc.payload, status_code=exc.status_code)

        return JSONResponse(content=response_payload, headers={"x-session-id": session_id})

    return app


app = create_app()
