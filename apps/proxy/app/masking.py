from __future__ import annotations

import copy
import re
from dataclasses import dataclass, field
from typing import Any

from app.detectors import Detector
PLACEHOLDER_PATTERN = re.compile(r"<<MASK:(?P<kind>[A-Z_]+)_(?P<index>\d+):MASK>>")
CONTENT_PART_TEXT_KEYS = frozenset({"text"})


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
        # Deduplication is intentionally keyed by raw value, not (kind, value), so repeated
        # occurrences of the same literal across the payload reuse a single placeholder.
        existing = self.real_to_placeholder.get(value)
        if existing:
            return existing

        next_index = self.counters.get(kind, 0) + 1
        self.counters[kind] = next_index

        placeholder = f"<<MASK:{kind}_{next_index}:MASK>>"
        self.real_to_placeholder[value] = placeholder
        self.placeholder_to_real[placeholder] = value
        return placeholder

def mask_text(text: str, state: MappingState, detector: Detector) -> str:
    # Known v1 limitation: existing <<MASK:...:MASK>> literals in user input are not
    # escaped before entity masking, so a later response echo could still be rehydrated.
    # Hardening should detect or escape those sequences before masking runs.
    matches = sorted(detector.detect(text), key=lambda match: match.start)
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


def mask_content_value(value: Any, state: MappingState, detector: Detector) -> Any:
    if isinstance(value, str):
        return mask_text(value, state, detector)

    if isinstance(value, list):
        masked_parts: list[Any] = []
        for item in value:
            if isinstance(item, str):
                masked_parts.append(mask_text(item, state, detector))
                continue

            if isinstance(item, dict):
                masked_part = copy.deepcopy(item)
                for key in CONTENT_PART_TEXT_KEYS:
                    field_value = masked_part.get(key)
                    if isinstance(field_value, str):
                        masked_part[key] = mask_text(field_value, state, detector)
                masked_parts.append(masked_part)
                continue

            masked_parts.append(item)
        return masked_parts

    return value


def mask_request_payload(
    payload: dict[str, Any],
    state: MappingState,
    detector: Detector,
) -> dict[str, Any]:
    masked_payload = copy.deepcopy(payload)
    masked_payload.pop("session_id", None)

    messages = masked_payload.get("messages")
    if isinstance(messages, list):
        for message in messages:
            if isinstance(message, dict) and "content" in message:
                message["content"] = mask_content_value(message.get("content"), state, detector)

    prompt = masked_payload.get("prompt")
    if isinstance(prompt, str):
        masked_payload["prompt"] = mask_text(prompt, state, detector)
    elif isinstance(prompt, list):
        masked_payload["prompt"] = [
            mask_text(item, state, detector) if isinstance(item, str) else item
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
