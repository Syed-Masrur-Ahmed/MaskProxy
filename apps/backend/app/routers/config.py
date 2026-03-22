from fastapi import APIRouter

from app.models import PrivacyConfig

router = APIRouter(prefix="/config", tags=["config"])

# In-memory store — replace with DB persistence in a later phase
_config = PrivacyConfig()


@router.get("", response_model=PrivacyConfig)
async def get_config() -> PrivacyConfig:
    return _config


@router.put("", response_model=PrivacyConfig)
async def update_config(payload: PrivacyConfig) -> PrivacyConfig:
    global _config
    _config = payload
    return _config
