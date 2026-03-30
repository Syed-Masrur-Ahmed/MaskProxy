from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, status
from pydantic import BaseModel
from sqlmodel import Session, select

from app.auth import create_access_token, get_current_user, hash_password, verify_password
from app.database import get_session
from app.models import User

router = APIRouter(prefix="/users", tags=["users"])


class UserProfile(BaseModel):
    email: str
    created_at: str
    access_token: str | None = None


class UpdateEmailRequest(BaseModel):
    email: str
    password: str


class UpdatePasswordRequest(BaseModel):
    current_password: str
    new_password: str


@router.get("/me", response_model=UserProfile)
def get_me(current_user: Annotated[User, Depends(get_current_user)]) -> UserProfile:
    return UserProfile(
        email=current_user.email,
        created_at=current_user.created_at.isoformat(),
    )


@router.patch("/me", response_model=UserProfile)
def update_email(
    body: UpdateEmailRequest,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> UserProfile:
    if not verify_password(body.password, current_user.hashed_password):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Incorrect password")

    if body.email == current_user.email:
        return UserProfile(email=current_user.email, created_at=current_user.created_at.isoformat())

    existing = session.exec(select(User).where(User.email == body.email)).first()
    if existing:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already in use")

    current_user.email = body.email
    session.add(current_user)
    session.commit()
    session.refresh(current_user)

    new_token = create_access_token(current_user.email)
    return UserProfile(
        email=current_user.email,
        created_at=current_user.created_at.isoformat(),
        access_token=new_token,
    )


@router.patch("/me/password", status_code=status.HTTP_204_NO_CONTENT)
def update_password(
    body: UpdatePasswordRequest,
    current_user: Annotated[User, Depends(get_current_user)],
    session: Annotated[Session, Depends(get_session)],
) -> None:
    if not verify_password(body.current_password, current_user.hashed_password):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Current password is incorrect")

    if len(body.new_password) < 8:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Password must be at least 8 characters")

    current_user.hashed_password = hash_password(body.new_password)
    session.add(current_user)
    session.commit()
