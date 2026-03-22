from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from app.routers import config

app = FastAPI(title="MaskProxy", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],
    allow_methods=["GET", "PUT"],
    allow_headers=["Content-Type"],
)

app.include_router(config.router)
