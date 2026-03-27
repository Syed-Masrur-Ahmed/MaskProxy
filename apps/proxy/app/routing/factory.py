from __future__ import annotations

from pathlib import Path

from app.config import Settings
from app.routing.embedding import OnnxTextEmbeddingProvider
from app.routing.base import Router
from app.routing.keyword import ConfigurableKeywordRouter
from app.routing.semantic import EmbeddingProvider, RouteExampleStore, SemanticRouter
from app.routing.store import build_in_memory_route_store, build_lancedb_route_store

ROUTING_PRESET_DIRS: dict[str, str] = {
    "all-MiniLM-L6-v2": "optimum-all-MiniLM-L6-v2",
    "optimum/all-MiniLM-L6-v2": "optimum-all-MiniLM-L6-v2",
}


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def _resolve_routing_artifact_paths(settings: Settings) -> tuple[str, str, str]:
    preset_dir_name = ROUTING_PRESET_DIRS.get(settings.routing_embedding_model_id)
    preset_dir = _repo_root() / "models" / preset_dir_name if preset_dir_name else None

    model_path = settings.routing_embedding_model_path
    tokenizer_path = settings.routing_embedding_tokenizer_path
    examples_path = settings.routing_examples_path

    if preset_dir is not None:
        model_path = model_path or str(preset_dir / "model.onnx")
        tokenizer_path = tokenizer_path or str(preset_dir / "tokenizer.json")
        examples_path = examples_path or str(preset_dir / "routes.json")

    return model_path, tokenizer_path, examples_path


def build_router(
    settings: Settings,
    *,
    semantic_embedding_provider: EmbeddingProvider | None = None,
    semantic_route_store: RouteExampleStore | None = None,
) -> Router:
    keyword_router = ConfigurableKeywordRouter(
        local_keywords=(
            settings.routing_local_keywords.split(",")
            if settings.routing_enabled
            else []
        ),
        default_target=settings.routing_default_target,
    )

    if not settings.routing_enabled or settings.routing_strategy == "keyword":
        return keyword_router

    if settings.routing_strategy == "semantic":
        resolved_embedding_provider = semantic_embedding_provider or build_embedding_provider(settings)
        resolved_route_store = semantic_route_store or build_route_store(
            settings,
            embedding_provider=resolved_embedding_provider,
        )

        return SemanticRouter(
            embedding_provider=resolved_embedding_provider,
            route_store=resolved_route_store,
            similarity_threshold=settings.routing_similarity_threshold,
            default_target=settings.routing_default_target,
            top_k=settings.routing_top_k,
        )

    raise ValueError(f"Unsupported routing strategy: {settings.routing_strategy}")


def build_embedding_provider(settings: Settings) -> EmbeddingProvider:
    if settings.routing_embedding_backend == "onnx":
        model_path, tokenizer_path, _ = _resolve_routing_artifact_paths(settings)
        providers = [
            provider.strip()
            for provider in settings.routing_embedding_providers.split(",")
            if provider.strip()
        ] or ["CPUExecutionProvider"]
        return OnnxTextEmbeddingProvider(
            model_path=model_path,
            tokenizer_path=tokenizer_path,
            providers=providers,
        )

    raise ValueError(f"Unsupported routing embedding backend: {settings.routing_embedding_backend}")


def build_route_store(
    settings: Settings,
    *,
    embedding_provider: EmbeddingProvider,
) -> RouteExampleStore:
    _, _, examples_path = _resolve_routing_artifact_paths(settings)

    if settings.routing_store_backend == "memory":
        return build_in_memory_route_store(examples_path, embedding_provider)
    if settings.routing_store_backend == "lancedb":
        return build_lancedb_route_store(
            db_uri=settings.routing_lancedb_uri,
            table_name=settings.routing_lancedb_table,
            examples_path=examples_path,
            embedding_provider=embedding_provider,
        )

    raise ValueError(f"Unsupported routing store backend: {settings.routing_store_backend}")
