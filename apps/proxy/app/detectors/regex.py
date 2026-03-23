from __future__ import annotations

import re

from app.detectors.base import EntityMatch, PRIORITY_REGEX


ENTITY_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("EMAIL", re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.IGNORECASE)),
    ("PHONE", re.compile(r"(?:(?<=\D)|^)(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?){2}\d{4}(?=\D|$)")),
    ("SSN", re.compile(r"\b\d{3}-\d{2}-\d{4}\b")),
)


class RegexDetector:
    def __init__(self, priority: int = PRIORITY_REGEX) -> None:
        self._priority = priority

    def detect(self, text: str) -> list[EntityMatch]:
        matches: list[EntityMatch] = []
        for kind, pattern in ENTITY_PATTERNS:
            for match in pattern.finditer(text):
                matches.append(
                    EntityMatch(
                        start=match.start(),
                        end=match.end(),
                        kind=kind,
                        value=match.group(0),
                        priority=self._priority,
                        source="regex",
                    )
                )
        return matches
