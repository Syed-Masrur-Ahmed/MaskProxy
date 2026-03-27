from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import Protocol


class RouteTarget(StrEnum):
    CLOUD = "cloud"
    LOCAL = "local"


@dataclass(frozen=True)
class RouteDecision:
    target: RouteTarget
    reason: str
    matched_keywords: tuple[str, ...] = field(default_factory=tuple)
    matched_example: str | None = None
    score: float | None = None


class Router(Protocol):
    # Text passed to route() is raw and unmasked. All routing backends must run locally
    # and must not send prompt text to external services.
    def route(self, text: str) -> RouteDecision:
        ...
