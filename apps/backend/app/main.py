import logging
from contextlib import asynccontextmanager

import redis.asyncio as aioredis
from fastapi import FastAPI, HTTPException

logging.basicConfig(level=logging.INFO)
from fastapi.middleware.cors import CORSMiddleware

from app.cache import REDIS_URL, RedisDep
from app.database import create_db_and_tables
from app.routers import auth, config, api_keys, logs, provider_keys, users


@asynccontextmanager
async def lifespan(app: FastAPI):
    create_db_and_tables()
    app.state.redis = aioredis.from_url(REDIS_URL, decode_responses=False)
    yield
    await app.state.redis.aclose()


app = FastAPI(title="MaskProxy", version="0.1.0", lifespan=lifespan)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_methods=["GET", "POST", "PATCH", "DELETE"],
    allow_headers=["Content-Type", "Authorization"],
)

app.include_router(auth.router)
app.include_router(config.router)
app.include_router(api_keys.router)
app.include_router(logs.router)
app.include_router(provider_keys.router)
app.include_router(users.router)


@app.get("/test-redis")
async def test_redis(redis: RedisDep) -> dict:
    try:
        await redis.set("test_key", "Redis is working!")
        value = await redis.get("test_key")
        return {"test_key": value.decode() if isinstance(value, bytes) else value}
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc))
