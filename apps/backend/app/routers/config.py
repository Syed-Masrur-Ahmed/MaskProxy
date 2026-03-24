import logging
from typing import Annotated

from fastapi import APIRouter, Depends
from sqlmodel import Session, select

from app.auth import get_current_user
from app.cache import RedisDep, get_privacy_config_cache, set_privacy_config_cache
from app.database import get_session
from app.models import PrivacyConfig, PrivacyConfigUpdate, User

logger = logging.getLogger(__name__)
router = APIRouter(prefix="/config", tags=["config"])


@router.get("", response_model=PrivacyConfig)
async def get_config(
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
    redis: RedisDep,
) -> PrivacyConfig:
    cached = await get_privacy_config_cache(redis, current_user.id)
    if cached is not None:
        logger.info("config cache hit  user_id=%s", current_user.id)
        return cached

    logger.info("config cache miss user_id=%s", current_user.id)
    config = session.exec(
        select(PrivacyConfig).where(PrivacyConfig.user_id == current_user.id)
    ).one()
    await set_privacy_config_cache(redis, config)
    return config


@router.patch("", response_model=PrivacyConfig)
async def update_config(
    payload: PrivacyConfigUpdate,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
    redis: RedisDep,
) -> PrivacyConfig:
    config = session.exec(
        select(PrivacyConfig).where(PrivacyConfig.user_id == current_user.id)
    ).one()

    for key, value in payload.model_dump(exclude_unset=True).items():
        setattr(config, key, value)

    session.add(config)
    session.commit()
    session.refresh(config)

    await set_privacy_config_cache(redis, config)

    return config
