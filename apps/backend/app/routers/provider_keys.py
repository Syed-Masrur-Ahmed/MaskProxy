from datetime import datetime
from typing import Annotated
from uuid import UUID

import httpx
from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel
from sqlmodel import Session, select

from app.auth import get_current_user
from app.database import get_session
from app.models import APIKey, ProviderKey, User
from app.security import hash_api_key, KEY_PREFIX
from app.security_utils import decrypt_key, encrypt_key

router = APIRouter(prefix="/v1/provider-keys", tags=["provider-keys"])

_VALIDATION_TIMEOUT = 8.0  # seconds


def _validate_key(provider_name: str, raw_key: str) -> None:
    """
    Makes a lightweight read request to the provider to confirm the key is valid.
    Raises HTTP 422 if the key is rejected, HTTP 502 if the provider is unreachable.
    """
    try:
        if provider_name == "OpenAI":
            r = httpx.get(
                "https://api.openai.com/v1/models",
                headers={"Authorization": f"Bearer {raw_key}"},
                timeout=_VALIDATION_TIMEOUT,
            )
        elif provider_name == "Anthropic":
            r = httpx.get(
                "https://api.anthropic.com/v1/models",
                headers={
                    "x-api-key": raw_key,
                    "anthropic-version": "2023-06-01",
                },
                timeout=_VALIDATION_TIMEOUT,
            )
        elif provider_name == "Gemini":
            r = httpx.get(
                "https://generativelanguage.googleapis.com/v1beta/models",
                params={"key": raw_key},
                timeout=_VALIDATION_TIMEOUT,
            )
        else:
            # Unknown provider — skip validation rather than block.
            return
    except httpx.TimeoutException:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=f"Timed out reaching {provider_name} to validate key.",
        )
    except httpx.RequestError as exc:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=f"Could not reach {provider_name}: {exc}",
        )

    if r.status_code in (401, 403):
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail=f"Invalid {provider_name} API key.",
        )
    if not r.is_success:
        raise HTTPException(
            status_code=status.HTTP_502_BAD_GATEWAY,
            detail=f"{provider_name} returned an unexpected status: {r.status_code}.",
        )


class CreateProviderKeyRequest(BaseModel):
    provider_name: str
    raw_key: str


class ProviderKeyResponse(BaseModel):
    id: UUID
    provider_name: str
    key_peek: str
    created_at: datetime


@router.post("", status_code=status.HTTP_201_CREATED, response_model=ProviderKeyResponse)
def add_provider_key(
    body: CreateProviderKeyRequest,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> ProviderKeyResponse:
    _validate_key(body.provider_name, body.raw_key)

    provider_key = ProviderKey(
        user_id=current_user.id,
        provider_name=body.provider_name,
        encrypted_key=encrypt_key(body.raw_key),
        key_peek=body.raw_key[-4:],
    )
    session.add(provider_key)
    session.commit()
    session.refresh(provider_key)

    return ProviderKeyResponse(
        id=provider_key.id,
        provider_name=provider_key.provider_name,
        key_peek=provider_key.key_peek,
        created_at=provider_key.created_at,
    )


@router.get("", response_model=list[ProviderKeyResponse])
def list_provider_keys(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> list[ProviderKeyResponse]:
    keys = session.exec(
        select(ProviderKey).where(ProviderKey.user_id == current_user.id)
    ).all()
    return [
        ProviderKeyResponse(
            id=k.id,
            provider_name=k.provider_name,
            key_peek=k.key_peek,
            created_at=k.created_at,
        )
        for k in keys
    ]


_PROVIDER_NAME_MAP = {
    "openai": "OpenAI",
    "anthropic": "Anthropic",
    "gemini": "Gemini",
}


from fastapi import Header


@router.get("/resolve")
def resolve_provider_key_for_proxy(
    provider: str,
    session: Annotated[Session, Depends(get_session)],
    authorization: Annotated[str, Header()] = "",
) -> dict:
    """Internal endpoint called by the proxy to fetch the decrypted provider key.

    Authenticates via mp_ API key (not JWT).
    """
    if not authorization.startswith("Bearer " + KEY_PREFIX):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid API key")

    raw_key = authorization[len("Bearer "):]
    hashed = hash_api_key(raw_key)
    api_key_row = session.exec(
        select(APIKey).where(APIKey.hashed_key == hashed)
    ).first()
    if api_key_row is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="API key not found")

    provider_name = _PROVIDER_NAME_MAP.get(provider.lower(), provider)
    provider_key = session.exec(
        select(ProviderKey).where(
            ProviderKey.user_id == api_key_row.user_id,
            ProviderKey.provider_name == provider_name,
        )
    ).first()
    if provider_key is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"No {provider_name} key configured",
        )

    return {"api_key": decrypt_key(provider_key.encrypted_key)}


@router.delete("/{key_id}", status_code=status.HTTP_204_NO_CONTENT)
def delete_provider_key(
    key_id: UUID,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> None:
    provider_key = session.exec(
        select(ProviderKey).where(
            ProviderKey.id == key_id,
            ProviderKey.user_id == current_user.id,
        )
    ).first()
    if provider_key is None:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Key not found")

    session.delete(provider_key)
    session.commit()
