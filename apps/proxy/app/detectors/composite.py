from __future__ import annotations

from app.detectors.base import Detector, EntityMatch


def resolve_overlaps(matches: list[EntityMatch]) -> list[EntityMatch]:
    ranked_matches = sorted(
        matches,
        key=lambda match: (
            -match.span_length,
            match.priority,
            match.start,
            match.end,
            match.kind,
        ),
    )

    selected: list[EntityMatch] = []
    for candidate in ranked_matches:
        if any(candidate.start < chosen.end and chosen.start < candidate.end for chosen in selected):
            continue
        selected.append(candidate)

    return sorted(selected, key=lambda match: (match.start, match.end, match.priority, match.kind))


class CompositeDetector:
    def __init__(self, detectors: list[Detector]) -> None:
        self._detectors = list(detectors)

    def detect(self, text: str) -> list[EntityMatch]:
        text_length = len(text)
        matches: list[EntityMatch] = []
        for detector in self._detectors:
            for match in detector.detect(text):
                if match.start < 0 or match.end > text_length or match.start >= match.end:
                    continue
                matches.append(match)
        return resolve_overlaps(matches)
