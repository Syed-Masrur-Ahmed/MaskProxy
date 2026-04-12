import json
from typing import Annotated

from fastapi import APIRouter, Depends, Header, HTTPException, Query, status
from pydantic import BaseModel
from sqlmodel import Session, select

from app.auth import get_current_user
from app.database import get_session
from app.models import APIKey, RequestLog, User
from app.security import hash_api_key, KEY_PREFIX

router = APIRouter(prefix="/v1/logs", tags=["logs"])

MAX_MASKED_PROMPT_LEN = 200


# ── Request / Response schemas ─────────────────────────────────────────────────

class CreateLogEntry(BaseModel):
    session_id: str
    provider: str
    model: str
    pii_detected_count: int = 0
    pii_types: list[str] = []
    route: str
    latency_ms: int = 0
    masked_prompt: str = ""
    status_code: int = 200


# ── POST /v1/logs — proxy ingestion (API key auth) ────────────────────────────

@router.post("", status_code=status.HTTP_201_CREATED)
def create_log_entry(
    body: CreateLogEntry,
    session: Annotated[Session, Depends(get_session)],
    authorization: Annotated[str, Header()] = "",
):
    """Ingest a request log from the proxy. Authenticates via mp_ API key."""
    if not authorization.startswith("Bearer " + KEY_PREFIX):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid API key")

    raw_key = authorization[len("Bearer "):]
    hashed = hash_api_key(raw_key)
    api_key_row = session.exec(
        select(APIKey).where(APIKey.hashed_key == hashed)
    ).first()
    if api_key_row is None:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="API key not found")

    log = RequestLog(
        user_id=api_key_row.user_id,
        session_id=body.session_id,
        provider=body.provider,
        model=body.model,
        pii_detected_count=body.pii_detected_count,
        pii_types=json.dumps(body.pii_types),
        route=body.route,
        latency_ms=body.latency_ms,
        masked_prompt=body.masked_prompt[:MAX_MASKED_PROMPT_LEN],
        status_code=body.status_code,
    )
    session.add(log)
    session.commit()
    session.refresh(log)
    return {"id": str(log.id), "status": "created"}
