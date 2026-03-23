from typing import Annotated

from fastapi import APIRouter, Depends
from sqlmodel import Session, select

from app.auth import get_current_user
from app.database import get_session
from app.models import PrivacyConfig, PrivacyConfigUpdate, User

router = APIRouter(prefix="/config", tags=["config"])


@router.get("", response_model=PrivacyConfig)
def get_config(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> PrivacyConfig:
    return session.exec(
        select(PrivacyConfig).where(PrivacyConfig.user_id == current_user.id)
    ).one()


@router.patch("", response_model=PrivacyConfig)
def update_config(
    payload: PrivacyConfigUpdate,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> PrivacyConfig:
    config = session.exec(
        select(PrivacyConfig).where(PrivacyConfig.user_id == current_user.id)
    ).one()

    for key, value in payload.model_dump(exclude_unset=True).items():
        setattr(config, key, value)

    session.add(config)
    session.commit()
    session.refresh(config)
    return config
