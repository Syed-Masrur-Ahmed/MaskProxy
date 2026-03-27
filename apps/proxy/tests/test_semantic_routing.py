from __future__ import annotations

import pytest

from app.config import Settings
from app.routing import (
    InMemoryRouteExampleStore,
    RouteExample,
    RouteTarget,
    SemanticRouter,
)
from app.routing.factory import build_router


class FakeEmbeddingProvider:
    def __init__(self, mapping: dict[str, list[float]]) -> None:
        self._mapping = mapping

    def embed(self, text: str) -> list[float]:
        return list(self._mapping[text])


def build_settings(**overrides: object) -> Settings:
    values = {
        "redis_url": "redis://cache:6379/0",
        "upstream_base_url": "http://upstream.test",
        "mapping_ttl_seconds": 900,
        "max_body_bytes": 1_048_576,
        "routing_enabled": True,
        "routing_strategy": "semantic",
        "routing_default_target": "cloud",
        "routing_similarity_threshold": 0.8,
        "routing_top_k": 3,
    }
    values.update(overrides)
    return Settings(**values)


def test_semantic_router_selects_best_matching_route() -> None:
    provider = FakeEmbeddingProvider(
        {
            "Summarize this patient discharge note": [1.0, 0.0],
            "Explain the capital of Peru": [0.0, 1.0],
            "Classify this medical document": [0.95, 0.05],
        }
    )
    store = InMemoryRouteExampleStore(
        [
            RouteExample(text="Summarize this patient discharge note", target=RouteTarget.LOCAL),
            RouteExample(text="Explain the capital of Peru", target=RouteTarget.CLOUD),
        ],
        provider,
    )
    router = SemanticRouter(
        embedding_provider=provider,
        route_store=store,
        similarity_threshold=0.8,
    )

    decision = router.route("Classify this medical document")

    assert decision.target == RouteTarget.LOCAL
    assert decision.reason == "semantic_match"
    assert decision.matched_example == "Summarize this patient discharge note"
    assert decision.score is not None
    assert decision.score > 0.99


def test_semantic_router_falls_back_when_similarity_is_too_low() -> None:
    provider = FakeEmbeddingProvider(
        {
            "Financial risk assessment": [1.0, 0.0],
            "General knowledge question": [0.0, 1.0],
            "Ambiguous prompt": [0.6, 0.6],
        }
    )
    store = InMemoryRouteExampleStore(
        [
            RouteExample(text="Financial risk assessment", target=RouteTarget.LOCAL),
            RouteExample(text="General knowledge question", target=RouteTarget.CLOUD),
        ],
        provider,
    )
    router = SemanticRouter(
        embedding_provider=provider,
        route_store=store,
        similarity_threshold=0.95,
    )

    decision = router.route("Ambiguous prompt")

    assert decision.target == RouteTarget.CLOUD
    assert decision.reason == "semantic_default_target"
    assert decision.score is not None
    assert decision.score < 0.95


def test_in_memory_route_store_add_examples_makes_new_examples_queryable() -> None:
    provider = FakeEmbeddingProvider(
        {
            "Financial risk assessment": [1.0, 0.0],
            "General knowledge question": [0.0, 1.0],
            "Analyze this financial statement": [1.0, 0.0],
        }
    )
    store = InMemoryRouteExampleStore([], provider)
    store.add_examples(
        [
            RouteExample(text="Financial risk assessment", target=RouteTarget.LOCAL),
            RouteExample(text="General knowledge question", target=RouteTarget.CLOUD),
        ]
    )
    router = SemanticRouter(
        embedding_provider=provider,
        route_store=store,
        similarity_threshold=0.8,
    )

    decision = router.route("Analyze this financial statement")

    assert decision.target == RouteTarget.LOCAL
    assert decision.matched_example == "Financial risk assessment"


def test_build_router_requires_semantic_artifact_config() -> None:
    settings = build_settings()

    with pytest.raises(ValueError, match="model_path"):
        build_router(settings)
