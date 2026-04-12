import uuid
from datetime import datetime, timezone
from typing import Optional
from uuid import UUID

from sqlmodel import Field, SQLModel


class User(SQLModel, table=True):
    __tablename__ = "users"

    id: UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    email: str = Field(unique=True, index=True)
    hashed_password: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PrivacyConfig(SQLModel, table=True):
    __tablename__ = "privacy_configs"

    id: Optional[int] = Field(default=None, primary_key=True)
    user_id: UUID = Field(foreign_key="users.id", unique=True, index=True)
    mask_names: bool = Field(default=True)
    mask_locations: bool = Field(default=True)
    mask_finance: bool = Field(default=True)
    threshold: float = Field(default=0.75)


class PrivacyConfigUpdate(SQLModel):
    mask_names: Optional[bool] = None
    mask_locations: Optional[bool] = None
    mask_finance: Optional[bool] = None
    threshold: Optional[float] = None


class ProviderKey(SQLModel, table=True):
    __tablename__ = "provider_keys"

    id: UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    user_id: UUID = Field(foreign_key="users.id", index=True)
    provider_name: str  # e.g. 'OpenAI', 'Anthropic'
    encrypted_key: bytes
    key_peek: str  # last 4 chars of the raw key, for display
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class RequestLog(SQLModel, table=True):
    __tablename__ = "request_logs"

    id: UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    user_id: UUID = Field(foreign_key="users.id", index=True)
    session_id: str = Field(index=True)
    timestamp: datetime = Field(default_factory=lambda: datetime.now(timezone.utc), index=True)
    provider: str
    model: str
    pii_detected_count: int = Field(default=0)
    pii_types: str = Field(default="[]")  # JSON-encoded list, e.g. '["EMAIL","SSN"]'
    route: str  # "cloud" or "local"
    latency_ms: int = Field(default=0)
    masked_prompt: str = Field(default="")
    status_code: int = Field(default=200)


class APIKey(SQLModel, table=True):
    __tablename__ = "api_keys"

    id: UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    user_id: UUID = Field(foreign_key="users.id", index=True)
    name: str
    key_peek: str  # last 4 chars of the raw key, for display
    hashed_key: str = Field(index=True)
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
