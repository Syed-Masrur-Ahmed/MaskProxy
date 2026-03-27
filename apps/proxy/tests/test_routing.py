from __future__ import annotations

import pytest

from app.routing import ConfigurableKeywordRouter, RouteTarget, extract_routable_text
from app.routing.store import load_route_examples


def test_keyword_router_routes_sensitive_keywords_local() -> None:
    router = ConfigurableKeywordRouter(local_keywords=["medical", "financial"])

    decision = router.route("Summarize this medical note.")

    assert decision.target == RouteTarget.LOCAL
    assert decision.reason == "matched_local_keyword"
    assert decision.matched_keywords == ("medical",)


def test_keyword_router_falls_back_to_default_target() -> None:
    router = ConfigurableKeywordRouter(local_keywords=["medical"], default_target=RouteTarget.CLOUD)

    decision = router.route("What is the capital of Peru?")

    assert decision.target == RouteTarget.CLOUD
    assert decision.reason == "default_target"
    assert decision.matched_keywords == ()


def test_extract_routable_text_collects_message_and_prompt_strings() -> None:
    payload = {
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Patient note"},
                    "Follow up tomorrow",
                ],
            }
        ],
        "prompt": ["Classify this", "Urgent"],
    }

    assert extract_routable_text(payload) == "Patient note\nFollow up tomorrow\nClassify this\nUrgent"


def test_load_route_examples_skips_invalid_entries(tmp_path) -> None:
    path = tmp_path / "routes.json"
    path.write_text(
        """
        [
          {"text": "Patient discharge summary", "target": "local"},
          {"text": "", "target": "local"},
          {"text": "Broken target", "target": "something_else"},
          42,
          {"target": "cloud"},
          {"text": "General trivia question", "target": "cloud"}
        ]
        """,
        encoding="utf-8",
    )

    examples = load_route_examples(str(path))

    assert [example.text for example in examples] == [
        "Patient discharge summary",
        "General trivia question",
    ]


def test_load_route_examples_rejects_non_list_root(tmp_path) -> None:
    path = tmp_path / "routes-root.json"
    path.write_text('{"text": "bad"}', encoding="utf-8")

    with pytest.raises(ValueError, match="JSON list"):
        load_route_examples(str(path))
