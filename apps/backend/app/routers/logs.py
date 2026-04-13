import json
from datetime import datetime, timezone
from typing import Annotated

from fastapi import APIRouter, Depends, Header, HTTPException, Query, status
from pydantic import BaseModel
from sqlmodel import Session, select, func, case, col

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


# ── GET /v1/logs — dashboard query (JWT auth) ─────────────────────────────────

class LogEntryResponse(BaseModel):
    id: str
    session_id: str
    timestamp: str
    provider: str
    model: str
    pii_detected_count: int
    pii_types: list[str]
    route: str
    latency_ms: int
    masked_prompt: str
    status_code: int


@router.get("", response_model=list[LogEntryResponse])
def list_logs(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
    limit: int = Query(default=50, le=200, ge=1),
    offset: int = Query(default=0, ge=0),
):
    """Retrieve request logs for the authenticated user."""
    logs = session.exec(
        select(RequestLog)
        .where(RequestLog.user_id == current_user.id)
        .order_by(RequestLog.timestamp.desc())  # type: ignore[union-attr]
        .offset(offset)
        .limit(limit)
    ).all()

    return [
        LogEntryResponse(
            id=str(log.id),
            session_id=log.session_id,
            timestamp=log.timestamp.isoformat(),
            provider=log.provider,
            model=log.model,
            pii_detected_count=log.pii_detected_count,
            pii_types=json.loads(log.pii_types),
            route=log.route,
            latency_ms=log.latency_ms,
            masked_prompt=log.masked_prompt,
            status_code=log.status_code,
        )
        for log in logs
    ]


# ── GET /v1/logs/stats — aggregated dashboard stats (JWT auth) ────────────────

class StatsResponse(BaseModel):
    requests_today: int
    pii_entities_masked: int
    avg_latency_ms: float
    local_route_pct: float


@router.get("/stats", response_model=StatsResponse)
def get_stats(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
):
    """Return aggregated stats for the current user for today (UTC)."""
    today_start = datetime.now(timezone.utc).replace(hour=0, minute=0, second=0, microsecond=0)

    row = session.exec(
        select(
            func.count().label("total"),
            func.coalesce(func.sum(col(RequestLog.pii_detected_count)), 0).label("pii_total"),
            func.coalesce(func.avg(col(RequestLog.latency_ms)), 0).label("avg_latency"),
            func.coalesce(
                func.sum(case((col(RequestLog.route) == "local", 1), else_=0)),
                0,
            ).label("local_count"),
        ).where(
            RequestLog.user_id == current_user.id,
            col(RequestLog.timestamp) >= today_start,
        )
    ).first()

    if row is None or row[0] == 0:
        return StatsResponse(
            requests_today=0,
            pii_entities_masked=0,
            avg_latency_ms=0.0,
            local_route_pct=0.0,
        )

    total, pii_total, avg_latency, local_count = row
    return StatsResponse(
        requests_today=total,
        pii_entities_masked=int(pii_total),
        avg_latency_ms=round(float(avg_latency), 1),
        local_route_pct=round(100.0 * int(local_count) / total, 1),
    )
