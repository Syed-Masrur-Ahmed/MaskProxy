import hashlib
import hmac
import secrets

from sqlmodel import Session, select

from app.models import APIKey

KEY_PREFIX = "mp_"


def generate_api_key() -> str:
    """Generate a secure random API key with a recognisable prefix."""
    return KEY_PREFIX + secrets.token_urlsafe(32)


def hash_api_key(raw_key: str) -> str:
    """Return a hex-encoded SHA-256 digest of the raw key."""
    return hashlib.sha256(raw_key.encode()).hexdigest()


def validate_api_key(raw_key: str, session: Session) -> APIKey | None:
    """
    Look up and return the APIKey row if the raw key is valid, else None.

    Uses a constant-time comparison to prevent timing attacks.
    Only keys that start with the expected prefix are even queried.
    """
    if not raw_key.startswith(KEY_PREFIX):
        return None

    candidate_hash = hash_api_key(raw_key)
    api_key = session.exec(
        select(APIKey).where(APIKey.hashed_key == candidate_hash)
    ).first()

    if api_key is None:
        return None

    # Extra constant-time guard: compare stored hash against candidate hash.
    if not hmac.compare_digest(api_key.hashed_key, candidate_hash):
        return None

    return api_key
