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


class APIKey(SQLModel, table=True):
    __tablename__ = "api_keys"

    id: UUID = Field(default_factory=uuid.uuid4, primary_key=True)
    user_id: UUID = Field(foreign_key="users.id", index=True)
    name: str
    key_peek: str  # last 4 chars of the raw key, for display
    hashed_key: str = Field(index=True)
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
