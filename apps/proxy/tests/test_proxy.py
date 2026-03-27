from __future__ import annotations

from typing import Any

import httpx
import pytest
from fastapi import FastAPI, Request
from fastapi.responses import PlainTextResponse, Response

from app.config import Settings
from app.detectors import NerPrediction
from app.detectors.regex import RegexDetector
from app.main import create_app
from app.masking import MappingState
from app.proxy_service import ProxyService
from app.routing.keyword import ConfigurableKeywordRouter
from app.routing.semantic import InMemoryRouteExampleStore, RouteExample, SemanticRouter
from app.routing.base import RouteTarget
from app.session_store import InMemorySessionStore, SessionStoreError


class SpySessionStore(InMemorySessionStore):
    def __init__(self) -> None:
        super().__init__()
        self.saved_ttls: list[int] = []

    async def save_mapping(self, session_id: str, mapping: dict[str, str], ttl_seconds: int) -> None:
        self.saved_ttls.append(ttl_seconds)
        await super().save_mapping(session_id, mapping, ttl_seconds)


class FailingSessionStore:
    async def save_mapping(self, session_id: str, mapping: dict[str, str], ttl_seconds: int) -> None:
        raise SessionStoreError("save failed")

    async def get_mapping(self, session_id: str) -> dict[str, str]:
        raise SessionStoreError("get failed")

    async def close(self) -> None:
        return None


class FakeNerBackend:
    def __init__(self, predictions: list[NerPrediction]) -> None:
        self._predictions = predictions

    def predict(self, text: str) -> list[NerPrediction]:
        return list(self._predictions)


class FakeEmbeddingProvider:
    def __init__(self, mapping: dict[str, list[float]]) -> None:
        self._mapping = mapping

    def embed(self, text: str) -> list[float]:
        return list(self._mapping[text])


def build_settings(**overrides: Any) -> Settings:
    values = {
        "redis_url": "redis://cache:6379/0",
        "upstream_base_url": "http://upstream.test",
        "local_upstream_base_url": "",
        "mapping_ttl_seconds": 900,
        "max_body_bytes": 1_048_576,
        "routing_enabled": False,
        "routing_default_target": "cloud",
        "routing_local_keywords": "",
    }
    values.update(overrides)
    return Settings(
        **values,
    )


@pytest.mark.asyncio
async def test_masks_request_and_rehydrates_response() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {
            "choices": [
                {
                    "message": {
                        "content": (
                            "Contact <<MASK:EMAIL_1:MASK>> or <<MASK:PHONE_1:MASK>>. "
                            "SSN on file: <<MASK:SSN_1:MASK>>."
                        )
                    }
                }
            ]
        }

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            headers={"x-session-id": "session-123"},
            json={
                "model": "gpt-4.1-mini",
                "messages": [
                    {
                        "role": "user",
                        "content": "Email alice@example.com or call 415-555-2671. SSN 123-45-6789.",
                    }
                ],
            },
        )

    assert response.status_code == 200
    assert captured["body"]["messages"][0]["content"] == (
        "Email <<MASK:EMAIL_1:MASK>> or call <<MASK:PHONE_1:MASK>>. "
        "SSN <<MASK:SSN_1:MASK>>."
    )
    assert response.json()["choices"][0]["message"]["content"] == (
        "Contact alice@example.com or 415-555-2671. SSN on file: 123-45-6789."
    )
    assert response.headers["x-session-id"] == "session-123"


@pytest.mark.asyncio
async def test_runtime_detector_factory_can_add_ner_masking() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {
            "choices": [
                {
                    "message": {
                        "content": "Hello <<MASK:PERSON_NAME_1:MASK>> at <<MASK:EMAIL_1:MASK>>"
                    }
                }
            ]
        }

    app = create_app(
        settings=build_settings(ner_enabled=True, ner_confidence_threshold=0.8),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
        ner_backend=FakeNerBackend(
            [NerPrediction(start=0, end=10, label="PER", score=0.97)]
        ),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "messages": [
                    {
                        "role": "user",
                        "content": "John Smith email alice@example.com",
                    }
                ]
            },
        )

    assert response.status_code == 200
    assert captured["body"]["messages"][0]["content"] == (
        "<<MASK:PERSON_NAME_1:MASK>> email <<MASK:EMAIL_1:MASK>>"
    )
    assert response.json()["choices"][0]["message"]["content"] == "Hello John Smith at alice@example.com"


@pytest.mark.asyncio
async def test_local_route_bypasses_masking_and_uses_local_upstream() -> None:
    cloud_upstream = FastAPI()
    local_captured: dict[str, Any] = {}
    local_upstream = FastAPI()

    @cloud_upstream.post("/v1/chat/completions")
    async def cloud_chat(_: Request) -> dict[str, Any]:
        pytest.fail("cloud upstream should not be used for local-routed traffic")

    @local_upstream.post("/v1/chat/completions")
    async def local_chat(request: Request) -> dict[str, Any]:
        local_captured["body"] = await request.json()
        return {"choices": [{"message": {"content": "Handled locally"}}]}

    app = create_app(
        settings=build_settings(
            routing_enabled=True,
            routing_local_keywords="medical,patient",
            local_upstream_base_url="http://local-upstream.test",
        ),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=cloud_upstream),
        local_upstream_transport=httpx.ASGITransport(app=local_upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "messages": [
                    {
                        "role": "user",
                        "content": "Summarize this patient note for Alice Johnson at alice@example.com",
                    }
                ]
            },
        )

    assert response.status_code == 200
    assert local_captured["body"]["messages"][0]["content"] == (
        "Summarize this patient note for Alice Johnson at alice@example.com"
    )
    assert response.json()["choices"][0]["message"]["content"] == "Handled locally"
    assert "x-session-id" not in response.headers


@pytest.mark.asyncio
async def test_local_route_without_local_upstream_returns_503() -> None:
    app = create_app(
        settings=build_settings(
            routing_enabled=True,
            routing_local_keywords="medical",
            local_upstream_base_url="",
        ),
        session_store=SpySessionStore(),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "Summarize this medical note"}]},
        )

    assert response.status_code == 503
    assert response.json()["detail"] == "Local route selected but no local upstream is configured."


@pytest.mark.asyncio
async def test_semantic_router_routes_to_local_upstream_without_masking() -> None:
    cloud_upstream = FastAPI()
    local_captured: dict[str, Any] = {}
    local_upstream = FastAPI()

    @cloud_upstream.post("/v1/chat/completions")
    async def cloud_chat(_: Request) -> dict[str, Any]:
        pytest.fail("cloud upstream should not be used for semantic local-routed traffic")

    @local_upstream.post("/v1/chat/completions")
    async def local_chat(request: Request) -> dict[str, Any]:
        local_captured["body"] = await request.json()
        return {"choices": [{"message": {"content": "Semantic local route"}}]}

    provider = FakeEmbeddingProvider(
        {
            "Summarize this patient discharge note": [1.0, 0.0],
            "General trivia question": [0.0, 1.0],
            "Please summarize this patient discharge note for Alice Johnson at alice@example.com": [0.98, 0.02],
        }
    )
    store = InMemoryRouteExampleStore(
        [
            RouteExample(text="Summarize this patient discharge note", target=RouteTarget.LOCAL),
            RouteExample(text="General trivia question", target=RouteTarget.CLOUD),
        ],
        provider,
    )
    router = SemanticRouter(
        embedding_provider=provider,
        route_store=store,
        similarity_threshold=0.8,
    )

    app = create_app(
        settings=build_settings(
            routing_enabled=True,
            routing_strategy="semantic",
            local_upstream_base_url="http://local-upstream.test",
        ),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=cloud_upstream),
        local_upstream_transport=httpx.ASGITransport(app=local_upstream),
        router=router,
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "messages": [
                    {
                        "role": "user",
                        "content": "Please summarize this patient discharge note for Alice Johnson at alice@example.com",
                    }
                ]
            },
        )

    assert response.status_code == 200
    assert local_captured["body"]["messages"][0]["content"] == (
        "Please summarize this patient discharge note for Alice Johnson at alice@example.com"
    )
    assert response.json()["choices"][0]["message"]["content"] == "Semantic local route"
    assert "x-session-id" not in response.headers


def test_warns_when_local_upstream_does_not_look_private(caplog: pytest.LogCaptureFixture) -> None:
    caplog.set_level("WARNING")

    service = ProxyService(
        settings=build_settings(local_upstream_base_url="https://api.openai.com"),
        session_store=SpySessionStore(),
        detector=RegexDetector(),
        router=ConfigurableKeywordRouter(local_keywords=[]),
    )

    assert "does not look local/private" in caplog.text

    import asyncio

    asyncio.run(service.close())


@pytest.mark.asyncio
async def test_forwards_only_allowed_headers_and_overrides_authorization() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["headers"] = dict(request.headers)
        return {"ok": True}

    app = create_app(
        settings=build_settings(upstream_api_key="server-secret"),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            headers={
                "authorization": "Bearer client-secret",
                "accept": "application/json",
                "content-type": "application/json",
                "cookie": "session=abc",
                "x-session-id": "session-123",
                "x-forwarded-for": "1.2.3.4",
            },
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 200
    assert captured["headers"]["authorization"] == "Bearer server-secret"
    assert captured["headers"]["accept"] == "application/json"
    assert "cookie" not in captured["headers"]
    assert "x-session-id" not in captured["headers"]
    assert "x-forwarded-for" not in captured["headers"]


@pytest.mark.asyncio
async def test_body_session_id_is_used_for_state_but_not_forwarded_upstream() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "session_id": "body-session",
                "messages": [{"role": "user", "content": "alice@example.com"}],
            },
        )

    assert response.status_code == 200
    assert response.headers["x-session-id"] == "body-session"
    assert "session_id" not in captured["body"]


@pytest.mark.asyncio
async def test_masks_content_fields_only_and_leaves_stop_untouched() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    payload = {
        "model": "gpt-4.1-mini",
        "stop": "leave alice@example.com alone",
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Primary alice@example.com"},
                    {"type": "text", "text": "Backup alice@example.com"},
                    {"type": "text", "text": "Call (415) 555-2671"},
                ],
            }
        ],
    }

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post("/v1/chat/completions", json=payload)

    assert response.status_code == 200
    content = captured["body"]["messages"][0]["content"]
    assert content[0]["text"] == "Primary <<MASK:EMAIL_1:MASK>>"
    assert content[1]["text"] == "Backup <<MASK:EMAIL_1:MASK>>"
    assert content[2]["text"] == "Call <<MASK:PHONE_1:MASK>>"
    assert captured["body"]["stop"] == "leave alice@example.com alone"


@pytest.mark.asyncio
async def test_prompt_list_strings_are_masked() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"prompt": ["Email alice@example.com", "Call 415-555-2671"]},
        )

    assert response.status_code == 200
    assert captured["body"]["prompt"] == [
        "Email <<MASK:EMAIL_1:MASK>>",
        "Call <<MASK:PHONE_1:MASK>>",
    ]


@pytest.mark.asyncio
async def test_existing_session_mapping_is_reused_and_extended() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()
    store = SpySessionStore()
    await store.save_mapping(
        "shared-session",
        {"<<MASK:EMAIL_1:MASK>>": "alice@example.com"},
        ttl_seconds=900,
    )

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {
            "choices": [
                {
                    "message": {
                        "content": "Stored <<MASK:EMAIL_1:MASK>>, new <<MASK:EMAIL_2:MASK>>."
                    }
                }
            ]
        }

    app = create_app(
        settings=build_settings(),
        session_store=store,
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            headers={"x-session-id": "shared-session"},
            json={
                "messages": [
                    {
                        "role": "user",
                        "content": "alice@example.com and bob@example.com",
                    }
                ]
            },
        )

    assert response.status_code == 200
    assert captured["body"]["messages"][0]["content"] == "<<MASK:EMAIL_1:MASK>> and <<MASK:EMAIL_2:MASK>>"
    assert response.json()["choices"][0]["message"]["content"] == "Stored alice@example.com, new bob@example.com."


@pytest.mark.asyncio
async def test_literal_square_bracket_placeholder_text_is_not_rehydrated() -> None:
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        return {
            "choices": [
                {
                    "message": {
                        "content": "[EMAIL_1] should stay literal; <<MASK:EMAIL_1:MASK>> should expand."
                    }
                }
            ]
        }

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 200
    assert response.json()["choices"][0]["message"]["content"] == (
        "[EMAIL_1] should stay literal; alice@example.com should expand."
    )


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "session_id",
    [
        "",
        "../escape",
        "bad/session",
        "bad\x00value",
        "x" * 129,
    ],
)
async def test_invalid_session_ids_are_rejected(session_id: str) -> None:
    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=FastAPI()),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "session_id": session_id,
                "messages": [{"role": "user", "content": "alice@example.com"}],
            },
        )

    assert response.status_code == 400
    assert response.json()["detail"] == "Invalid session ID."


@pytest.mark.asyncio
async def test_upstream_error_status_and_body_are_preserved_and_rehydrated() -> None:
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request):
        from fastapi.responses import JSONResponse

        return JSONResponse(
            status_code=429,
            content={"error": {"message": "Too many requests for <<MASK:EMAIL_1:MASK>>"}},
        )

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 429
    assert response.json()["error"]["message"] == "Too many requests for alice@example.com"


@pytest.mark.asyncio
async def test_upstream_non_json_error_body_uses_text_fallback() -> None:
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request):
        return PlainTextResponse("gateway blew up", status_code=502)

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 502
    assert response.json()["error"] == "gateway blew up"


@pytest.mark.asyncio
async def test_upstream_non_json_success_body_returns_502() -> None:
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request):
        return PlainTextResponse("<html>ok?</html>", status_code=200)

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 502
    assert response.json()["error"] == "Upstream returned non-JSON success response: <html>ok?</html>"


@pytest.mark.asyncio
async def test_upstream_empty_success_body_returns_502() -> None:
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request):
        return Response(content=b"", media_type="text/plain", status_code=200)

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 502
    assert response.json()["error"] == "Upstream returned non-JSON success response: "


@pytest.mark.asyncio
async def test_rejects_oversized_body() -> None:
    app = create_app(
        settings=build_settings(max_body_bytes=64),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=FastAPI()),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            content='{"messages":[{"role":"user","content":"' + ("x" * 128) + '"}]}',
            headers={"content-type": "application/json"},
        )

    assert response.status_code == 413
    assert response.json()["detail"] == "Request body too large."


@pytest.mark.asyncio
async def test_rejects_content_length_over_limit_before_body_parse() -> None:
    app = create_app(
        settings=build_settings(max_body_bytes=64),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=FastAPI()),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            content="{",
            headers={"content-type": "application/json", "content-length": "128"},
        )

    assert response.status_code == 413
    assert response.json()["detail"] == "Request body too large."


@pytest.mark.asyncio
async def test_prompt_only_and_null_message_content_do_not_crash() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={
                "prompt": "Contact alice@example.com",
                "messages": [{"role": "system", "content": None}],
            },
        )

    assert response.status_code == 200
    assert captured["body"]["prompt"] == "Contact <<MASK:EMAIL_1:MASK>>"
    assert captured["body"]["messages"][0]["content"] is None


@pytest.mark.asyncio
async def test_message_without_content_key_does_not_crash() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "assistant"}]},
        )

    assert response.status_code == 200
    assert captured["body"]["messages"] == [{"role": "assistant"}]


@pytest.mark.asyncio
async def test_null_body_session_id_generates_request_session_id() -> None:
    captured: dict[str, Any] = {}
    upstream = FastAPI()

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        captured["body"] = await request.json()
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"session_id": None, "messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 200
    assert response.headers["x-session-id"].startswith("req-")
    assert "session_id" not in captured["body"]


@pytest.mark.asyncio
async def test_non_string_body_session_id_is_rejected() -> None:
    app = create_app(
        settings=build_settings(),
        session_store=SpySessionStore(),
        upstream_transport=httpx.ASGITransport(app=FastAPI()),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"session_id": 123, "messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 400
    assert response.json()["detail"] == "Invalid session ID."


def test_mapping_state_restores_multiword_entity_counters() -> None:
    state = MappingState.from_placeholder_mapping({"<<MASK:PERSON_NAME_2:MASK>>": "Alice Example"})
    assert state.placeholder_for("PERSON_NAME", "Bob Example") == "<<MASK:PERSON_NAME_3:MASK>>"


@pytest.mark.asyncio
async def test_fails_closed_when_session_store_is_unavailable() -> None:
    upstream = FastAPI()
    hit_upstream = {"value": False}

    @upstream.post("/v1/chat/completions")
    async def upstream_chat(request: Request) -> dict[str, Any]:
        hit_upstream["value"] = True
        return {"ok": True}

    app = create_app(
        settings=build_settings(),
        session_store=FailingSessionStore(),
        upstream_transport=httpx.ASGITransport(app=upstream),
    )

    async with httpx.AsyncClient(transport=httpx.ASGITransport(app=app), base_url="http://testserver") as client:
        response = await client.post(
            "/v1/chat/completions",
            json={"messages": [{"role": "user", "content": "alice@example.com"}]},
        )

    assert response.status_code == 503
    assert response.json()["detail"] == "PII session state unavailable; refusing to forward masked request."
    assert hit_upstream["value"] is False
