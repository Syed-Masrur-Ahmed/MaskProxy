from __future__ import annotations

from typing import Any

from app.routing.base import RouteDecision, RouteTarget


class ConfigurableKeywordRouter:
    def __init__(
        self,
        *,
        local_keywords: list[str],
        default_target: RouteTarget = RouteTarget.CLOUD,
    ) -> None:
        self._local_keywords = [keyword.strip().lower() for keyword in local_keywords if keyword.strip()]
        self._default_target = default_target

    def route(self, text: str) -> RouteDecision:
        normalized_text = text.lower()
        matched_keywords = tuple(
            keyword for keyword in self._local_keywords
            if keyword in normalized_text
        )
        if matched_keywords:
            return RouteDecision(
                target=RouteTarget.LOCAL,
                reason="matched_local_keyword",
                matched_keywords=matched_keywords,
            )

        return RouteDecision(target=self._default_target, reason="default_target")


def extract_routable_text(payload: dict[str, Any]) -> str:
    parts: list[str] = []

    messages = payload.get("messages")
    if isinstance(messages, list):
        for message in messages:
            if not isinstance(message, dict):
                continue
            content = message.get("content")
            parts.extend(_flatten_content(content))

    prompt = payload.get("prompt")
    if isinstance(prompt, str):
        parts.append(prompt)
    elif isinstance(prompt, list):
        parts.extend(item for item in prompt if isinstance(item, str))

    return "\n".join(part for part in parts if part)


def _flatten_content(content: Any) -> list[str]:
    if isinstance(content, str):
        return [content]

    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, str):
                parts.append(item)
            elif isinstance(item, dict):
                text = item.get("text")
                if isinstance(text, str):
                    parts.append(text)
        return parts

    return []
