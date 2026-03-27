from __future__ import annotations

from pathlib import Path
from typing import Any, Protocol

from app.routing.base import RouteTarget
from app.routing.semantic import RouteExample, RouteExampleStore, RouteMatch


class LanceTable(Protocol):
    def add(self, data: list[dict[str, Any]]) -> Any:
        ...

    def search(self, vector: list[float]) -> Any:
        ...


class LanceSearchResult(Protocol):
    def limit(self, value: int) -> "LanceSearchResult":
        ...

    def to_list(self) -> list[dict[str, Any]]:
        ...


class LanceDbConnection(Protocol):
    def create_table(
        self,
        name: str,
        data: list[dict[str, Any]] | None = None,
        mode: str = "create",
        schema: Any | None = None,
    ) -> LanceTable:
        ...

    def open_table(self, name: str) -> LanceTable:
        ...

    def table_names(self) -> list[str]:
        ...


class LanceDbRouteStore(RouteExampleStore):
    def __init__(
        self,
        *,
        db_uri: str,
        table_name: str,
        connection: LanceDbConnection,
    ) -> None:
        self._db_uri = db_uri
        self._table_name = table_name
        self._connection = connection
        self._table = self._ensure_table()

    def add_examples(self, examples: list[RouteExample]) -> None:
        raise NotImplementedError(
            "LanceDbRouteStore expects pre-embedded rows via rebuild_from_embeddings(); "
            "upsert semantics will be added when dashboard-driven writes exist."
        )

    def rebuild_from_embeddings(self, rows: list[dict[str, Any]]) -> None:
        self._table = self._connection.create_table(self._table_name, data=rows, mode="overwrite")

    def query(self, embedding: list[float], *, limit: int) -> list[RouteMatch]:
        raw_rows = self._table.search(embedding).limit(limit).to_list()
        matches: list[RouteMatch] = []
        for row in raw_rows:
            text = row.get("text")
            target = row.get("target")
            if (
                not isinstance(text, str)
                or not text.strip()
                or target not in {RouteTarget.CLOUD.value, RouteTarget.LOCAL.value}
            ):
                continue

            # LanceDB returns _distance (lower = more similar). Convert to cosine
            # similarity (higher = more similar) so the SemanticRouter threshold
            # works identically for both in-memory and LanceDB stores.
            raw_distance = row.get("_distance")
            if isinstance(raw_distance, (int, float)):
                similarity = 1.0 - float(raw_distance)
            else:
                raw_score = row.get("score", row.get("_score", 0.0))
                if not isinstance(raw_score, (int, float)):
                    continue
                similarity = float(raw_score)

            matches.append(
                RouteMatch(
                    example=RouteExample(text=text, target=RouteTarget(target)),
                    score=similarity,
                )
            )
        return matches

    def _ensure_table(self) -> LanceTable:
        if self._table_name in self._connection.table_names():
            return self._connection.open_table(self._table_name)
        try:
            return self._connection.create_table(self._table_name, data=[], mode="create")
        except Exception as exc:
            schema = _empty_route_examples_schema()
            if schema is None:
                raise RuntimeError(
                    "Failed to create an empty LanceDB table and could not build a fallback schema. "
                    "Install pyarrow or upgrade LanceDB."
                ) from exc
            return self._connection.create_table(self._table_name, mode="create", schema=schema)


def _empty_route_examples_schema() -> Any | None:
    try:
        import pyarrow as pa
    except ImportError:
        return None

    return pa.schema(
        [
            pa.field("text", pa.string()),
            pa.field("target", pa.string()),
            pa.field("vector", pa.list_(pa.float32())),
        ]
    )


def connect_lancedb(db_uri: str) -> LanceDbConnection:
    try:
        import lancedb
    except ImportError as exc:  # pragma: no cover
        raise RuntimeError("lancedb is required for ROUTING_STORE_BACKEND=lancedb.") from exc

    Path(db_uri).mkdir(parents=True, exist_ok=True)
    return lancedb.connect(db_uri)
