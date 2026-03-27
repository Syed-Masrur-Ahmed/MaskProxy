from __future__ import annotations

from dataclasses import dataclass
from math import sqrt
from typing import Protocol

from app.routing.base import RouteDecision, RouteTarget


class EmbeddingProvider(Protocol):
    def embed(self, text: str) -> list[float]:
        ...


@dataclass(frozen=True)
class RouteExample:
    text: str
    target: RouteTarget


@dataclass(frozen=True)
class RouteMatch:
    example: RouteExample
    score: float


class RouteExampleStore(Protocol):
    def add_examples(self, examples: list[RouteExample]) -> None:
        ...

    def query(self, embedding: list[float], *, limit: int) -> list[RouteMatch]:
        ...


class InMemoryRouteExampleStore:
    def __init__(
        self,
        examples: list[RouteExample] | None,
        embedding_provider: EmbeddingProvider,
    ) -> None:
        self._examples: list[RouteExample] = []
        self._embedding_provider = embedding_provider
        self._embedded_examples: list[tuple[RouteExample, list[float]]] = []
        if examples:
            self.add_examples(examples)

    def add_examples(self, examples: list[RouteExample]) -> None:
        self._examples.extend(examples)
        self._embedded_examples.extend(
            (example, self._embedding_provider.embed(example.text))
            for example in examples
        )

    def query(self, embedding: list[float], *, limit: int) -> list[RouteMatch]:
        matches = [
            RouteMatch(example=example, score=_cosine_similarity(embedding, stored_embedding))
            for example, stored_embedding in self._embedded_examples
        ]
        matches.sort(key=lambda match: match.score, reverse=True)
        return matches[:limit]


class SemanticRouter:
    def __init__(
        self,
        *,
        embedding_provider: EmbeddingProvider,
        route_store: RouteExampleStore,
        similarity_threshold: float,
        default_target: RouteTarget = RouteTarget.CLOUD,
        top_k: int = 3,
    ) -> None:
        self._embedding_provider = embedding_provider
        self._route_store = route_store
        self._similarity_threshold = similarity_threshold
        self._default_target = default_target
        self._top_k = top_k

    def route(self, text: str) -> RouteDecision:
        if not text.strip():
            return RouteDecision(target=self._default_target, reason="semantic_default_target")

        embedding = self._embedding_provider.embed(text)
        matches = self._route_store.query(embedding, limit=self._top_k)
        if not matches:
            return RouteDecision(target=self._default_target, reason="semantic_default_target")

        best_match = matches[0]
        if best_match.score < self._similarity_threshold:
            return RouteDecision(
                target=self._default_target,
                reason="semantic_default_target",
                matched_example=best_match.example.text,
                score=best_match.score,
            )

        return RouteDecision(
            target=best_match.example.target,
            reason="semantic_match",
            matched_example=best_match.example.text,
            score=best_match.score,
        )


def _cosine_similarity(left: list[float], right: list[float]) -> float:
    if not left or not right or len(left) != len(right):
        return 0.0

    left_norm = sqrt(sum(value * value for value in left))
    right_norm = sqrt(sum(value * value for value in right))
    if left_norm == 0.0 or right_norm == 0.0:
        return 0.0

    dot_product = sum(left_value * right_value for left_value, right_value in zip(left, right))
    return dot_product / (left_norm * right_norm)
