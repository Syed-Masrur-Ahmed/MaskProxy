from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from app.config import Settings
from app.routing.base import RouteTarget
from app.routing.factory import build_route_store
from app.routing.lancedb_store import LanceDbRouteStore
from app.routing.semantic import RouteExample
from app.routing.store import build_lancedb_route_store


class StaticEmbeddingProvider:
    def __init__(self, mapping: dict[str, list[float]]) -> None:
        self._mapping = mapping

    def embed(self, text: str) -> list[float]:
        return list(self._mapping[text])


class FakeSearch:
    def __init__(self, rows: list[dict[str, Any]]) -> None:
        self._rows = rows
        self._limit = None

    def limit(self, value: int) -> "FakeSearch":
        self._limit = value
        return self

    def to_list(self) -> list[dict[str, Any]]:
        if self._limit is None:
            return list(self._rows)
        return list(self._rows[: self._limit])


class FakeTable:
    def __init__(self, rows: list[dict[str, Any]] | None = None) -> None:
        self.rows = rows or []
        self.added: list[dict[str, Any]] = []
        self.last_search_vector: list[float] | None = None

    def add(self, data: list[dict[str, Any]]) -> None:
        self.added.extend(data)
        self.rows.extend(data)

    def search(self, vector: list[float]) -> FakeSearch:
        self.last_search_vector = list(vector)
        # Real LanceDB returns results sorted by distance ascending (lowest = most similar)
        ranked = sorted(self.rows, key=self._distance_key)
        return FakeSearch(ranked)

    def _distance_key(self, row: dict[str, Any]) -> float:
        value = row.get("_distance", float("inf"))
        if isinstance(value, (int, float)):
            return float(value)
        return float("inf")


class FakeConnection:
    def __init__(self) -> None:
        self.tables: dict[str, FakeTable] = {}
        self.fail_empty_create = False
        self.last_create_schema: object | None = None

    def create_table(
        self,
        name: str,
        data: list[dict[str, Any]] | None = None,
        mode: str = "create",
        schema: object | None = None,
    ) -> FakeTable:
        if self.fail_empty_create and data == [] and schema is None:
            raise ValueError("empty data not supported")

        self.last_create_schema = schema
        table = FakeTable(list(data or []))
        self.tables[name] = table
        return table

    def open_table(self, name: str) -> FakeTable:
        return self.tables[name]

    def table_names(self) -> list[str]:
        return list(self.tables)


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
        "routing_embedding_backend": "onnx",
        "routing_embedding_model_id": "",
        "routing_embedding_model_path": "/models/embed/model.onnx",
        "routing_embedding_tokenizer_path": "/models/embed/tokenizer.json",
        "routing_embedding_providers": "CPUExecutionProvider",
        "routing_store_backend": "lancedb",
        "routing_examples_path": "/tmp/routes.json",
        "routing_lancedb_uri": "/tmp/lancedb",
        "routing_lancedb_table": "route_examples",
    }
    values.update(overrides)
    return Settings(**values)


def test_lancedb_route_store_queries_ranked_matches() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            # _distance 0.08 → similarity 0.92 (very similar)
            {"text": "Patient discharge summary", "target": "local", "_distance": 0.08},
            # _distance 0.79 → similarity 0.21 (not similar)
            {"text": "General trivia question", "target": "cloud", "_distance": 0.79},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0, 0.0], limit=1)

    assert len(matches) == 1
    assert matches[0].example.text == "Patient discharge summary"
    assert matches[0].example.target == RouteTarget.LOCAL
    assert matches[0].score == pytest.approx(0.92)


def test_lancedb_route_store_opens_existing_table_when_present() -> None:
    connection = FakeConnection()
    existing = connection.create_table(
        "route_examples",
        data=[{"text": "Existing row", "target": "cloud", "_distance": 0.5}],
    )

    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    assert store._table is existing


def test_lancedb_route_store_creates_empty_table_when_missing() -> None:
    connection = FakeConnection()

    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    assert "route_examples" in connection.tables
    assert store._table.rows == []


def test_lancedb_route_store_skips_blank_text_rows() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            {"text": "   ", "target": "cloud", "_distance": 0.05},
            {"text": "Valid row", "target": "local", "_distance": 0.2},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=10)

    assert len(matches) == 1
    assert matches[0].example.text == "Valid row"


def test_lancedb_route_store_falls_back_to_schema_when_empty_create_fails(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = FakeConnection()
    connection.fail_empty_create = True
    schema_sentinel = object()
    monkeypatch.setattr("app.routing.lancedb_store._empty_route_examples_schema", lambda: schema_sentinel)

    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    assert "route_examples" in connection.tables
    assert connection.last_create_schema is schema_sentinel
    assert store._table.rows == []


def test_lancedb_route_store_raises_when_empty_create_fails_and_no_schema_available(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    connection = FakeConnection()
    connection.fail_empty_create = True
    monkeypatch.setattr("app.routing.lancedb_store._empty_route_examples_schema", lambda: None)

    with pytest.raises(RuntimeError, match="Failed to create an empty LanceDB table"):
        LanceDbRouteStore(
            db_uri="/tmp/lancedb",
            table_name="route_examples",
            connection=connection,
        )


def test_lancedb_route_store_filters_invalid_rows_and_supports_score_fallbacks() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            # _distance 0.09 → similarity 0.91
            {"text": "Distance row", "target": "local", "_distance": 0.09},
            # No _distance, falls back to score field (used as-is, not inverted)
            {"text": "Score row", "target": "cloud", "score": 0.75},
            # No _distance or score, falls back to _score field
            {"text": "Score alias row", "target": "local", "_score": 0.66},
            # Empty text — should be filtered out
            {"text": "", "target": "cloud", "_distance": 0.12},
            # Invalid target — filtered out
            {"text": "Bad target", "target": "other", "_distance": 0.23},
            # Non-numeric _distance — falls through to score/_ score, none exist → skipped
            {"text": "Bad score", "target": "cloud", "_distance": "bad"},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([0.0, 1.0], limit=10)

    results = [(m.example.text, m.example.target, m.score) for m in matches]
    # FakeTable sorts by _distance ascending. Rows without numeric _distance sort last.
    # "Distance row" _distance=0.09 → similarity=0.91
    # "" _distance=0.12 → filtered (blank text)
    # "Bad target" _distance=0.23 → filtered (invalid target)
    # "Bad score" _distance="bad" (non-numeric) → fallback to score/_ score → 0.0
    # "Score row" no _distance → fallback to score=0.75
    # "Score alias row" no _distance → fallback to _score=0.66
    assert ("Distance row", RouteTarget.LOCAL, pytest.approx(0.91)) in results
    assert all(m.example.text != "" for m in matches)
    assert ("Score row", RouteTarget.CLOUD, 0.75) in results
    assert ("Score alias row", RouteTarget.LOCAL, 0.66) in results
    # "Bad target" should be filtered out
    assert all(m.example.text != "Bad target" for m in matches)


def test_lancedb_route_store_add_examples_is_not_supported() -> None:
    connection = FakeConnection()
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    with pytest.raises(NotImplementedError, match="pre-embedded rows"):
        store.add_examples([RouteExample(text="Patient discharge summary", target=RouteTarget.LOCAL)])


def test_build_lancedb_route_store_rebuilds_from_examples(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    routes_path = tmp_path / "routes.json"
    routes_path.write_text(
        json.dumps(
            [
                {"text": "Patient discharge summary", "target": "local"},
                {"text": "General trivia question", "target": "cloud"},
            ]
        ),
        encoding="utf-8",
    )
    connection = FakeConnection()
    provider = StaticEmbeddingProvider(
        {
            "Patient discharge summary": [1.0, 0.0],
            "General trivia question": [0.0, 1.0],
        }
    )

    monkeypatch.setattr("app.routing.store.connect_lancedb", lambda _: connection)

    store = build_lancedb_route_store(
        db_uri=str(tmp_path / "db"),
        table_name="route_examples",
        examples_path=str(routes_path),
        embedding_provider=provider,
    )

    assert isinstance(store, LanceDbRouteStore)
    table = connection.open_table("route_examples")
    assert table.rows == [
        {"text": "Patient discharge summary", "target": "local", "vector": [1.0, 0.0]},
        {"text": "General trivia question", "target": "cloud", "vector": [0.0, 1.0]},
    ]


def test_lancedb_route_store_rebuild_overwrites_existing_data() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[{"text": "Old row", "target": "cloud", "_distance": 0.5}],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    store.rebuild_from_embeddings(
        [
            {"text": "New row A", "target": "local", "vector": [1.0, 0.0], "_distance": 0.9},
            {"text": "New row B", "target": "cloud", "vector": [0.0, 1.0], "_distance": 0.3},
        ]
    )

    table = connection.open_table("route_examples")
    texts = [row["text"] for row in table.rows]
    assert "Old row" not in texts
    assert "New row A" in texts
    assert "New row B" in texts


def test_lancedb_route_store_query_empty_table_returns_no_matches() -> None:
    connection = FakeConnection()
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0, 0.0], limit=5)

    assert matches == []


def test_lancedb_route_store_query_respects_limit() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            {"text": f"Row {i}", "target": "cloud", "_distance": float(i) / 10}
            for i in range(10)
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=3)

    assert len(matches) == 3


def test_lancedb_route_store_query_with_missing_text_field_skips_row() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            {"target": "cloud", "_distance": 0.9},
            {"text": "Valid row", "target": "local", "_distance": 0.8},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=10)

    assert len(matches) == 1
    assert matches[0].example.text == "Valid row"


def test_lancedb_route_store_query_with_none_text_skips_row() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            {"text": None, "target": "cloud", "_distance": 0.9},
            {"text": "Good row", "target": "local", "_distance": 0.7},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=10)

    assert len(matches) == 1
    assert matches[0].example.text == "Good row"


def test_lancedb_route_store_query_with_no_score_fields_defaults_to_zero() -> None:
    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[{"text": "No score", "target": "local"}],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=10)

    assert len(matches) == 1
    assert matches[0].score == 0.0


def test_lancedb_route_store_query_converts_integer_distance() -> None:
    connection = FakeConnection()
    # _distance of 0 (integer) → similarity 1.0
    connection.create_table(
        "route_examples",
        data=[{"text": "Int distance", "target": "cloud", "_distance": 0}],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )

    matches = store.query([1.0], limit=10)

    assert len(matches) == 1
    assert matches[0].score == 1.0
    assert isinstance(matches[0].score, float)


def test_lancedb_distance_to_similarity_works_with_semantic_router_threshold() -> None:
    """The key integration test: a low _distance (very similar) should convert
    to a high similarity score that passes the SemanticRouter threshold."""
    from app.routing.semantic import SemanticRouter

    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            # _distance 0.05 → similarity 0.95 (should pass threshold of 0.8)
            {"text": "Patient discharge summary", "target": "local", "_distance": 0.05},
            # _distance 0.85 → similarity 0.15 (should NOT pass threshold)
            {"text": "General trivia", "target": "cloud", "_distance": 0.85},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )
    router = SemanticRouter(
        embedding_provider=StaticEmbeddingProvider({"test query": [1.0, 0.0]}),
        route_store=store,
        similarity_threshold=0.8,
    )

    decision = router.route("test query")

    assert decision.target == RouteTarget.LOCAL
    assert decision.reason == "semantic_match"
    assert decision.score == pytest.approx(0.95)


def test_lancedb_high_distance_falls_back_to_default_with_semantic_router() -> None:
    """When all results have high _distance (low similarity), the router
    should fall back to the default target."""
    from app.routing.semantic import SemanticRouter

    connection = FakeConnection()
    connection.create_table(
        "route_examples",
        data=[
            # _distance 0.9 → similarity 0.1 (below threshold)
            {"text": "Some local example", "target": "local", "_distance": 0.9},
        ],
    )
    store = LanceDbRouteStore(
        db_uri="/tmp/lancedb",
        table_name="route_examples",
        connection=connection,
    )
    router = SemanticRouter(
        embedding_provider=StaticEmbeddingProvider({"unrelated query": [0.0, 1.0]}),
        route_store=store,
        similarity_threshold=0.8,
        default_target=RouteTarget.CLOUD,
    )

    decision = router.route("unrelated query")

    assert decision.target == RouteTarget.CLOUD
    assert decision.reason == "semantic_default_target"
    assert decision.score == pytest.approx(0.1)


def test_build_lancedb_route_store_raises_on_empty_examples(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    routes_path = tmp_path / "routes.json"
    routes_path.write_text("[]", encoding="utf-8")
    connection = FakeConnection()
    monkeypatch.setattr("app.routing.store.connect_lancedb", lambda _: connection)

    with pytest.raises(ValueError, match="did not contain any valid examples"):
        build_lancedb_route_store(
            db_uri=str(tmp_path / "db"),
            table_name="route_examples",
            examples_path=str(routes_path),
            embedding_provider=StaticEmbeddingProvider({}),
        )


def test_build_route_store_supports_lancedb_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    captured: dict[str, Any] = {}

    class StaticProvider:
        def embed(self, text: str) -> list[float]:
            return [0.0]

    def fake_builder(*, db_uri: str, table_name: str, examples_path: str, embedding_provider: object) -> str:
        captured["db_uri"] = db_uri
        captured["table_name"] = table_name
        captured["examples_path"] = examples_path
        captured["embedding_provider"] = embedding_provider
        return "store"

    monkeypatch.setattr("app.routing.factory.build_lancedb_route_store", fake_builder)

    settings = build_settings(
        routing_embedding_model_id="all-MiniLM-L6-v2",
        routing_examples_path="",
        routing_lancedb_uri="data/lancedb",
        routing_lancedb_table="route_examples",
    )

    store = build_route_store(settings, embedding_provider=StaticProvider())

    assert store == "store"
    assert captured["db_uri"] == "data/lancedb"
    assert captured["table_name"] == "route_examples"
    assert str(captured["examples_path"]).endswith("/models/optimum-all-MiniLM-L6-v2/routes.json")
