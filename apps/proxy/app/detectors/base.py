from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

PRIORITY_REGEX = 0
PRIORITY_NER = 10


@dataclass(frozen=True)
class EntityMatch:
    start: int
    end: int
    kind: str
    value: str
    # Lower numbers mean higher priority when overlap lengths tie.
    priority: int = 100
    source: str = "unknown"

    @property
    def span_length(self) -> int:
        return self.end - self.start


class Detector(Protocol):
    # Detectors return raw matches. Callers should not assume sorted or de-overlapped output;
    # CompositeDetector is responsible for validation, sorting, and overlap resolution.
    def detect(self, text: str) -> list[EntityMatch]:
        ...
