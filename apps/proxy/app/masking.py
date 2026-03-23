from __future__ import annotations

import copy
import re
from dataclasses import dataclass, field
from typing import Any


ENTITY_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("EMAIL", re.compile(r"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b", re.IGNORECASE)),
    ("PHONE", re.compile(r"(?:(?<=\D)|^)(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?){2}\d{4}(?=\D|$)")),
    ("SSN", re.compile(r"\b\d{3}-\d{2}-\d{4}\b")),
)
PLACEHOLDER_PATTERN = re.compile(r"<<MASK:(?P<kind>[A-Z_]+)_(?P<index>\d+):MASK>>")
CONTENT_PART_TEXT_KEYS = frozenset({"text", "input_text"})


@dataclass(order=True)
class EntityMatch:
    start: int
    end: int
    kind: str
    value: str


@dataclass
class MappingState:
    counters: dict[str, int] = field(default_factory=dict)
    real_to_placeholder: dict[str, str] = field(default_factory=dict)
    placeholder_to_real: dict[str, str] = field(default_factory=dict)

    @classmethod
    def from_placeholder_mapping(cls, placeholder_to_real: dict[str, str]) -> "MappingState":
        state = cls()
        for placeholder, real_value in placeholder_to_real.items():
            state.placeholder_to_real[placeholder] = real_value
            state.real_to_placeholder[real_value] = placeholder

            match = PLACEHOLDER_PATTERN.fullmatch(placeholder)
            if not match:
                continue

            kind = match.group("kind")
            raw_index = match.group("index")
            state.counters[kind] = max(state.counters.get(kind, 0), int(raw_index))

        return state

    def placeholder_for(self, kind: str, value: str) -> str:
        existing = self.real_to_placeholder.get(value)
        if existing:
            return existing

        next_index = self.counters.get(kind, 0) + 1
        self.counters[kind] = next_index

        placeholder = f"<<MASK:{kind}_{next_index}:MASK>>"
        self.real_to_placeholder[value] = placeholder
        self.placeholder_to_real[placeholder] = value
        return placeholder


def _find_matches(text: str) -> list[EntityMatch]:
    matches: list[EntityMatch] = []
    for kind, pattern in ENTITY_PATTERNS:
        for match in pattern.finditer(text):
            matches.append(EntityMatch(match.start(), match.end(), kind, match.group(0)))

    matches.sort(key=lambda item: (item.start, -(item.end - item.start), item.kind))

    non_overlapping: list[EntityMatch] = []
    current_end = -1
    for match in matches:
        if match.start < current_end:
            continue
        non_overlapping.append(match)
        current_end = match.end

    return non_overlapping


def mask_text(text: str, state: MappingState) -> str:
    matches = _find_matches(text)
    if not matches:
        return text

    chunks: list[str] = []
    cursor = 0

    for match in matches:
        chunks.append(text[cursor:match.start])
        chunks.append(state.placeholder_for(match.kind, match.value))
        cursor = match.end

    chunks.append(text[cursor:])
    return "".join(chunks)


def mask_value(value: Any, state: MappingState) -> Any:
    if isinstance(value, str):
        return mask_text(value, state)

    if isinstance(value, list):
        return [mask_value(item, state) for item in value]

    if isinstance(value, dict):
        return {key: mask_value(item, state) for key, item in value.items()}

    return value


def mask_content_value(value: Any, state: MappingState) -> Any:
    if isinstance(value, str):
        return mask_text(value, state)

    if isinstance(value, list):
        masked_parts: list[Any] = []
        for item in value:
            if isinstance(item, str):
                masked_parts.append(mask_text(item, state))
                continue

            if isinstance(item, dict):
                masked_part = copy.deepcopy(item)
                for key in CONTENT_PART_TEXT_KEYS:
                    field_value = masked_part.get(key)
                    if isinstance(field_value, str):
                        masked_part[key] = mask_text(field_value, state)
                masked_parts.append(masked_part)
                continue

            masked_parts.append(item)
        return masked_parts

    return value


def mask_request_payload(payload: dict[str, Any], state: MappingState) -> dict[str, Any]:
    masked_payload = copy.deepcopy(payload)

    messages = masked_payload.get("messages")
    if isinstance(messages, list):
        for message in messages:
            if isinstance(message, dict) and "content" in message:
                message["content"] = mask_content_value(message.get("content"), state)

    prompt = masked_payload.get("prompt")
    if isinstance(prompt, str):
        masked_payload["prompt"] = mask_text(prompt, state)
    elif isinstance(prompt, list):
        masked_payload["prompt"] = [
            mask_text(item, state) if isinstance(item, str) else item
            for item in prompt
        ]

    return masked_payload


def rehydrate_value(value: Any, placeholder_to_real: dict[str, str]) -> Any:
    if isinstance(value, str):
        return PLACEHOLDER_PATTERN.sub(
            lambda match: placeholder_to_real.get(match.group(0), match.group(0)),
            value,
        )

    if isinstance(value, list):
        return [rehydrate_value(item, placeholder_to_real) for item in value]

    if isinstance(value, dict):
        return {key: rehydrate_value(item, placeholder_to_real) for key, item in value.items()}

    return value
