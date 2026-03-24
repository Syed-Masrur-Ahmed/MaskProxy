from __future__ import annotations

from dataclasses import dataclass
from typing import Protocol

from app.detectors.base import EntityMatch, PRIORITY_NER

DEFAULT_NER_LABEL_MAP: dict[str, str] = {
    "PER": "PERSON_NAME",
    "PERSON": "PERSON_NAME",
    "PERSON_NAME": "PERSON_NAME",
    "LOC": "LOCATION",
    "LOCATION": "LOCATION",
    "GPE": "LOCATION",
    "ORG": "ORGANIZATION",
    "ORGANIZATION": "ORGANIZATION",
}


@dataclass(frozen=True)
class NerPrediction:
    start: int
    end: int
    label: str
    score: float
    text: str = ""


class NerBackend(Protocol):
    def predict(self, text: str) -> list[NerPrediction]:
        ...


class NerDetector:
    def __init__(
        self,
        backend: NerBackend,
        confidence_threshold: float,
        label_map: dict[str, str] | None = None,
        priority: int = PRIORITY_NER,
    ) -> None:
        self._backend = backend
        self._confidence_threshold = confidence_threshold
        self._label_map = label_map or DEFAULT_NER_LABEL_MAP
        self._priority = priority

    def detect(self, text: str) -> list[EntityMatch]:
        matches: list[EntityMatch] = []
        for prediction in self._backend.predict(text):
            if prediction.score < self._confidence_threshold:
                continue

            normalized_kind = self._normalize_label(prediction.label)
            if normalized_kind is None:
                continue

            if prediction.start < 0 or prediction.end > len(text) or prediction.start >= prediction.end:
                continue

            matches.append(
                EntityMatch(
                    start=prediction.start,
                    end=prediction.end,
                    kind=normalized_kind,
                    value=text[prediction.start:prediction.end],
                    priority=self._priority,
                    source="ner",
                )
            )

        return matches

    def _normalize_label(self, label: str) -> str | None:
        normalized = label.upper()
        if "-" in normalized:
            prefix, _, suffix = normalized.partition("-")
            if prefix in {"B", "I", "L", "U", "E", "S"}:
                normalized = suffix
        return self._label_map.get(normalized)

