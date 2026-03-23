from functools import lru_cache

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        populate_by_name=True,
    )

    proxy_port: int = Field(8080, alias="PROXY_PORT")
    redis_url: str = Field("redis://cache:6379/0", alias="REDIS_URL")
    upstream_base_url: str = Field("https://api.openai.com", alias="UPSTREAM_BASE_URL")
    upstream_api_key: str | None = Field(None, alias="UPSTREAM_API_KEY")
    mapping_ttl_seconds: int = Field(3600, alias="MAPPING_TTL_SECONDS")
    max_body_bytes: int = Field(1_048_576, alias="MAX_BODY_BYTES")


@lru_cache
def get_settings() -> Settings:
    return Settings()
