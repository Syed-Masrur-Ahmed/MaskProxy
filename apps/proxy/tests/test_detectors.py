from __future__ import annotations

from dataclasses import dataclass

from app.detectors import (
    CompositeDetector,
    EntityMatch,
    PRIORITY_NER,
    PRIORITY_REGEX,
    RegexDetector,
    resolve_overlaps,
)
from app.masking import MappingState, mask_text


@dataclass
class MockDetector:
    matches: list[EntityMatch]

    def detect(self, text: str) -> list[EntityMatch]:
        return list(self.matches)


def test_resolve_overlaps_prefers_longest_span_across_detectors() -> None:
    matches = [
        EntityMatch(
            start=5,
            end=17,
            kind="PHONE",
            value="415-555-2671",
            priority=PRIORITY_REGEX,
            source="regex",
        ),
        EntityMatch(start=10, end=17, kind="PERSON", value="2671", priority=PRIORITY_NER, source="ner"),
    ]

    resolved = resolve_overlaps(matches)

    assert resolved == [
        EntityMatch(
            start=5,
            end=17,
            kind="PHONE",
            value="415-555-2671",
            priority=PRIORITY_REGEX,
            source="regex",
        )
    ]


def test_resolve_overlaps_uses_detector_priority_for_equal_spans() -> None:
    text = "alice@example.com"
    regex = RegexDetector()
    ner = MockDetector(
        [
            EntityMatch(
                start=0,
                end=len(text),
                kind="PERSON",
                value=text,
                priority=PRIORITY_NER,
                source="ner",
            )
        ]
    )

    resolved = CompositeDetector([regex, ner]).detect(text)

    assert resolved == [
        EntityMatch(
            start=0,
            end=len(text),
            kind="EMAIL",
            value="alice@example.com",
            priority=PRIORITY_REGEX,
            source="regex",
        )
    ]


def test_mask_text_can_use_mock_ner_detector_before_onnx_integration() -> None:
    text = "John called 415-555-2671"
    state = MappingState()
    detector = CompositeDetector(
        [
            RegexDetector(),
            MockDetector(
                [
                    EntityMatch(
                        start=0,
                        end=4,
                        kind="PERSON_NAME",
                        value="John",
                        priority=PRIORITY_NER,
                        source="ner",
                    ),
                ]
            ),
        ]
    )

    masked = mask_text(text, state, detector)

    assert masked == "<<MASK:PERSON_NAME_1:MASK>> called <<MASK:PHONE_1:MASK>>"


def test_documented_d1_behavior_keeps_broad_longer_span_over_inner_regex_match() -> None:
    text = "Alice Smith at alice@example.com"
    detector = CompositeDetector(
        [
            RegexDetector(),
            MockDetector(
                [
                    EntityMatch(
                        start=0,
                        end=len(text),
                        kind="PERSON_NAME",
                        value=text,
                        priority=PRIORITY_NER,
                        source="ner",
                    ),
                ]
            ),
        ]
    )

    # If this behavior becomes undesirable after real NER integration, the fix is a
    # carve-out strategy in resolve_overlaps, not changing the sort key.
    resolved = detector.detect(text)

    assert resolved == [
        EntityMatch(
            start=0,
            end=len(text),
            kind="PERSON_NAME",
            value=text,
            priority=PRIORITY_NER,
            source="ner",
        )
    ]


def test_invalid_detector_offsets_are_filtered_before_overlap_resolution() -> None:
    text = "alice@example.com"
    detector = CompositeDetector(
        [
            MockDetector(
                [
                    EntityMatch(start=-1, end=5, kind="BROKEN", value="bad", priority=PRIORITY_NER, source="ner"),
                    EntityMatch(start=0, end=0, kind="EMPTY", value="", priority=PRIORITY_NER, source="ner"),
                    EntityMatch(start=0, end=999, kind="TOO_LONG", value=text, priority=PRIORITY_NER, source="ner"),
                ]
            ),
            RegexDetector(),
        ]
    )

    resolved = detector.detect(text)

    assert resolved == [
        EntityMatch(
            start=0,
            end=len(text),
            kind="EMAIL",
            value="alice@example.com",
            priority=PRIORITY_REGEX,
            source="regex",
        )
    ]


def test_three_way_overlap_selection_is_stable() -> None:
    matches = [
        EntityMatch(start=0, end=10, kind="NER_A", value="abcdefghij", priority=PRIORITY_NER, source="ner-a"),
        EntityMatch(start=5, end=15, kind="PHONE", value="fghijklmno", priority=PRIORITY_REGEX, source="regex"),
        EntityMatch(start=12, end=20, kind="NER_B", value="mnopqrst", priority=PRIORITY_NER, source="ner-b"),
    ]

    resolved = resolve_overlaps(matches)

    assert resolved == [
        EntityMatch(start=5, end=15, kind="PHONE", value="fghijklmno", priority=PRIORITY_REGEX, source="regex"),
    ]


def test_adjacent_matches_are_both_kept() -> None:
    matches = [
        EntityMatch(start=0, end=10, kind="PHONE", value="1234567890", priority=PRIORITY_REGEX, source="regex"),
        EntityMatch(start=10, end=20, kind="SSN", value="123-45-6789", priority=PRIORITY_REGEX, source="regex"),
    ]

    resolved = resolve_overlaps(matches)

    assert resolved == matches


def test_same_literal_string_with_different_kinds_reuses_first_placeholder() -> None:
    text = "token token"
    state = MappingState()
    detector = CompositeDetector(
        [
            MockDetector(
                [
                    EntityMatch(start=0, end=5, kind="PHONE", value="token", priority=PRIORITY_REGEX, source="regex"),
                    EntityMatch(start=6, end=11, kind="SSN", value="token", priority=PRIORITY_NER, source="ner"),
                ]
            )
        ]
    )

    masked = mask_text(text, state, detector)

    assert masked == "<<MASK:PHONE_1:MASK>> <<MASK:PHONE_1:MASK>>"


def test_mask_text_sorts_direct_regex_detector_output_defensively() -> None:
    text = "Call 415-555-2671 or email alice@example.com"
    state = MappingState()

    masked = mask_text(text, state, RegexDetector())

    assert masked == "Call <<MASK:PHONE_1:MASK>> or email <<MASK:EMAIL_1:MASK>>"


def test_single_detector_overlaps_are_resolved_safely() -> None:
    text = "abcdefghij"
    detector = CompositeDetector(
        [
            MockDetector(
                [
                    EntityMatch(start=0, end=10, kind="BROAD", value=text, priority=PRIORITY_NER, source="mock"),
                    EntityMatch(start=2, end=5, kind="NARROW", value="cde", priority=PRIORITY_NER, source="mock"),
                ]
            )
        ]
    )

    resolved = detector.detect(text)

    assert resolved == [
        EntityMatch(start=0, end=10, kind="BROAD", value=text, priority=PRIORITY_NER, source="mock")
    ]


def test_empty_text_detector_path_returns_no_matches_and_no_crash() -> None:
    detector = CompositeDetector([RegexDetector()])

    assert detector.detect("") == []
