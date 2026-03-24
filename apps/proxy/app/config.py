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
    ner_enabled: bool = Field(False, alias="NER_ENABLED")
    ner_backend: str = Field("none", alias="NER_BACKEND")
    ner_model_id: str = Field("", alias="NER_MODEL_ID")
    ner_device: int = Field(-1, alias="NER_DEVICE")
    ner_hf_token: str | None = Field(None, alias="NER_HF_TOKEN")
    ner_onnx_model_path: str = Field("", alias="NER_ONNX_MODEL_PATH")
    ner_onnx_tokenizer_path: str = Field("", alias="NER_ONNX_TOKENIZER_PATH")
    ner_onnx_labels_path: str = Field("", alias="NER_ONNX_LABELS_PATH")
    ner_onnx_providers: str = Field("CPUExecutionProvider", alias="NER_ONNX_PROVIDERS")
    ner_confidence_threshold: float = Field(0.75, alias="NER_CONFIDENCE_THRESHOLD")


@lru_cache
def get_settings() -> Settings:
    return Settings()
