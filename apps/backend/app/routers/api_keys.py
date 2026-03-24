from datetime import datetime
from typing import Annotated
from uuid import UUID

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel
from sqlmodel import Session, select

from app.auth import get_current_user
from app.database import get_session
from app.models import APIKey, User
from app.security import generate_api_key, hash_api_key

router = APIRouter(prefix="/keys", tags=["api-keys"])


class CreateKeyRequest(BaseModel):
    name: str


class APIKeyCreatedResponse(BaseModel):
    id: UUID
    name: str
    key_peek: str
    created_at: datetime
    key: str  # plain-text key — returned exactly once


class APIKeyPublicResponse(BaseModel):
    id: UUID
    name: str
    key_peek: str
    created_at: datetime


@router.post("", status_code=status.HTTP_201_CREATED, response_model=APIKeyCreatedResponse)
def create_key(
    body: CreateKeyRequest,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> APIKeyCreatedResponse:
    raw_key = generate_api_key()
    api_key = APIKey(
        user_id=current_user.id,
        name=body.name,
        key_peek=raw_key[-4:],
        hashed_key=hash_api_key(raw_key),
    )
    session.add(api_key)
    session.commit()
    session.refresh(api_key)

    return APIKeyCreatedResponse(
        id=api_key.id,
        name=api_key.name,
        key_peek=api_key.key_peek,
        created_at=api_key.created_at,
        key=raw_key,
    )


@router.get("", response_model=list[APIKeyPublicResponse])
def list_keys(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> list[APIKeyPublicResponse]:
    keys = session.exec(
        select(APIKey).where(APIKey.user_id == current_user.id)
    ).all()
    return [
        APIKeyPublicResponse(
            id=k.id,
            name=k.name,
            key_peek=k.key_peek,
            created_at=k.created_at,
        )
        for k in keys
    ]


@router.delete("/{key_id}", status_code=status.HTTP_204_NO_CONTENT)
def revoke_key(
    key_id: UUID,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> None:
    api_key = session.exec(
        select(APIKey).where(APIKey.id == key_id, APIKey.user_id == current_user.id)
    ).first()
    if api_key is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Key not found")

    session.delete(api_key)
    session.commit()
