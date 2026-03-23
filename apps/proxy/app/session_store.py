from __future__ import annotations

import json
from typing import Protocol

from redis.asyncio import Redis


class SessionStoreError(RuntimeError):
    pass


class SessionStore(Protocol):
    async def save_mapping(self, session_id: str, mapping: dict[str, str], ttl_seconds: int) -> None:
        ...

    async def get_mapping(self, session_id: str) -> dict[str, str]:
        ...

    async def close(self) -> None:
        ...


class RedisSessionStore:
    def __init__(self, redis_url: str, key_prefix: str = "maskproxy:session") -> None:
        self._redis = Redis.from_url(redis_url, decode_responses=True)
        self._key_prefix = key_prefix

    def _key(self, session_id: str) -> str:
        return f"{self._key_prefix}:{session_id}"

    async def save_mapping(self, session_id: str, mapping: dict[str, str], ttl_seconds: int) -> None:
        try:
            await self._redis.set(self._key(session_id), json.dumps(mapping), ex=ttl_seconds)
        except Exception as exc:  # pragma: no cover - exercised in integration
            raise SessionStoreError("failed to persist session mapping") from exc

    async def get_mapping(self, session_id: str) -> dict[str, str]:
        try:
            payload = await self._redis.get(self._key(session_id))
        except Exception as exc:  # pragma: no cover - exercised in integration
            raise SessionStoreError("failed to load session mapping") from exc

        if not payload:
            return {}
        return json.loads(payload)

    async def close(self) -> None:
        await self._redis.aclose()


class InMemorySessionStore:
    def __init__(self) -> None:
        self._data: dict[str, dict[str, str]] = {}

    async def save_mapping(self, session_id: str, mapping: dict[str, str], ttl_seconds: int) -> None:
        # Test-only store: it intentionally ignores TTL and keeps data in memory.
        self._data[session_id] = dict(mapping)

    async def get_mapping(self, session_id: str) -> dict[str, str]:
        return dict(self._data.get(session_id, {}))

    async def close(self) -> None:
        return None
