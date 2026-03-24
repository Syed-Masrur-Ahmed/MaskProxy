import json
import os
from typing import Annotated, TypeAlias
from uuid import UUID

import redis.asyncio as aioredis
from fastapi import Depends, Request

from app.models import PrivacyConfig

REDIS_URL = os.environ.get("REDIS_URL", "redis://localhost:6379/0")

API_KEY_TTL = 300  # 5 minutes


def get_redis(request: Request) -> aioredis.Redis:
    return request.app.state.redis


RedisDep: TypeAlias = Annotated[aioredis.Redis, Depends(get_redis)]

# ---------------------------------------------------------------------------
# Privacy config cache  (key: privacy_config:<user_id>)
# ---------------------------------------------------------------------------

_CONFIG_KEY = "privacy_config:{}"


async def set_privacy_config_cache(redis: aioredis.Redis, config: PrivacyConfig) -> None:
    key = _CONFIG_KEY.format(config.user_id)
    await redis.set(key, config.model_dump_json())


async def get_privacy_config_cache(
    redis: aioredis.Redis, user_id: UUID
) -> PrivacyConfig | None:
    raw = await redis.get(_CONFIG_KEY.format(user_id))
    if raw is None:
        return None
    return PrivacyConfig.model_validate(json.loads(raw))


async def delete_privacy_config_cache(redis: aioredis.Redis, user_id: UUID) -> None:
    await redis.delete(_CONFIG_KEY.format(user_id))


# ---------------------------------------------------------------------------
# API key validation cache  (key: api_key_valid:<hashed_key>, TTL 5 min)
# Stores the owner's user_id string so the proxy can resolve the caller.
# ---------------------------------------------------------------------------

_API_KEY_VALID_KEY = "api_key_valid:{}"


async def set_api_key_validation_cache(
    redis: aioredis.Redis, hashed_key: str, user_id: UUID
) -> None:
    key = _API_KEY_VALID_KEY.format(hashed_key)
    await redis.set(key, str(user_id), ex=API_KEY_TTL)


async def get_api_key_validation_cache(
    redis: aioredis.Redis, hashed_key: str
) -> UUID | None:
    raw = await redis.get(_API_KEY_VALID_KEY.format(hashed_key))
    if raw is None:
        return None
    return UUID(raw.decode() if isinstance(raw, bytes) else raw)


async def delete_api_key_validation_cache(
    redis: aioredis.Redis, hashed_key: str
) -> None:
    await redis.delete(_API_KEY_VALID_KEY.format(hashed_key))
