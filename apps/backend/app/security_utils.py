import os

from cryptography.fernet import Fernet

_raw_key = os.environ["MASTER_ENCRYPTION_KEY"]
_fernet = Fernet(_raw_key.encode() if isinstance(_raw_key, str) else _raw_key)


def encrypt_key(raw_key: str) -> bytes:
    """Encrypt a provider API key. Returns opaque bytes suitable for DB storage."""
    return _fernet.encrypt(raw_key.encode())


def decrypt_key(encrypted_blob: bytes) -> str:
    """Decrypt a previously encrypted provider API key."""
    return _fernet.decrypt(encrypted_blob).decode()
