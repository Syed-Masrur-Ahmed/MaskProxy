from __future__ import annotations

from pathlib import Path

from app.routing.base import RouteTarget
from app.routing.lancedb_store import LanceDbRouteStore, connect_lancedb
from app.routing.semantic import InMemoryRouteExampleStore, RouteExample, EmbeddingProvider


def load_route_examples(path: str) -> list[RouteExample]:
    import json

    raw = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(raw, list):
        raise ValueError("Routing example file must contain a JSON list.")

    examples: list[RouteExample] = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        text = item.get("text")
        target = item.get("target")
        if not isinstance(text, str) or not text.strip():
            continue
        if target not in {RouteTarget.CLOUD.value, RouteTarget.LOCAL.value}:
            continue
        examples.append(RouteExample(text=text, target=RouteTarget(target)))

    if not examples:
        raise ValueError("Routing example file did not contain any valid examples.")

    return examples


def build_in_memory_route_store(
    examples_path: str,
    embedding_provider: EmbeddingProvider,
) -> InMemoryRouteExampleStore:
    return InMemoryRouteExampleStore(load_route_examples(examples_path), embedding_provider)


def build_lancedb_route_store(
    *,
    db_uri: str,
    table_name: str,
    examples_path: str,
    embedding_provider: EmbeddingProvider,
) -> LanceDbRouteStore:
    examples = load_route_examples(examples_path)
    store = LanceDbRouteStore(
        db_uri=db_uri,
        table_name=table_name,
        connection=connect_lancedb(db_uri),
    )
    store.rebuild_from_embeddings(
        [
            {
                "text": example.text,
                "target": example.target.value,
                "vector": embedding_provider.embed(example.text),
            }
            for example in examples
        ]
    )
    return store
