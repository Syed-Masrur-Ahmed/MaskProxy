from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest

from app.config import Settings
from app.routing.embedding import OnnxTextEmbeddingProvider
from app.routing.factory import _repo_root, build_embedding_provider, build_route_store, build_router
from app.routing.store import load_route_examples


class FakeEncoding:
    def __init__(self, ids: list[int], attention_mask: list[int], type_ids: list[int]) -> None:
        self.ids = ids
        self.attention_mask = attention_mask
        self.type_ids = type_ids


class FakeTokenizer:
    def __init__(self, encoding: FakeEncoding) -> None:
        self._encoding = encoding

    def encode(self, text: str) -> FakeEncoding:
        return self._encoding


class FakeInput:
    def __init__(self, name: str) -> None:
        self.name = name


class FakeSession:
    def __init__(self, outputs: list[Any], input_names: list[str] | None = None) -> None:
        self._outputs = outputs
        self._input_names = input_names or ["input_ids", "attention_mask", "token_type_ids"]

    def get_inputs(self) -> list[FakeInput]:
        return [FakeInput(name) for name in self._input_names]

    def run(self, output_names: list[str] | None, inputs: dict[str, Any]) -> list[Any]:
        return self._outputs


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
        "routing_store_backend": "memory",
        "routing_examples_path": "/tmp/routes.json",
    }
    values.update(overrides)
    return Settings(**values)


def test_onnx_text_embedding_provider_mean_pools_sequence_output() -> None:
    provider = OnnxTextEmbeddingProvider(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        tokenizer=FakeTokenizer(FakeEncoding(ids=[1, 2, 3], attention_mask=[1, 1, 0], type_ids=[0, 0, 0])),
        session=FakeSession([[[[1.0, 3.0], [3.0, 5.0], [100.0, 100.0]]]]),
        normalize=False,
    )

    assert provider.embed("hello") == [2.0, 4.0]


def test_onnx_text_embedding_provider_returns_empty_vector_for_blank_text() -> None:
    provider = OnnxTextEmbeddingProvider(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        tokenizer=FakeTokenizer(FakeEncoding(ids=[1], attention_mask=[1], type_ids=[0])),
        session=FakeSession([[[[1.0, 2.0]]]]),
    )

    assert provider.embed("   ") == []


def test_load_route_examples_reads_valid_examples(tmp_path: Path) -> None:
    path = tmp_path / "routes.json"
    path.write_text(
        json.dumps(
            [
                {"text": "Summarize this patient note", "target": "local"},
                {"text": "What is the capital of Peru?", "target": "cloud"},
            ]
        ),
        encoding="utf-8",
    )

    examples = load_route_examples(str(path))

    assert [example.text for example in examples] == [
        "Summarize this patient note",
        "What is the capital of Peru?",
    ]


def test_build_router_constructs_semantic_router_from_settings(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
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

    class FakeProvider:
        def __init__(self, model_path: str, tokenizer_path: str, providers: list[str]) -> None:
            self.model_path = model_path
            self.tokenizer_path = tokenizer_path
            self.providers = providers

        def embed(self, text: str) -> list[float]:
            return [1.0, 0.0] if "Patient" in text else [0.0, 1.0]

    monkeypatch.setattr("app.routing.factory.OnnxTextEmbeddingProvider", FakeProvider)

    settings = build_settings(routing_examples_path=str(routes_path))

    router = build_router(settings)

    decision = router.route("Patient billing note")

    assert decision.target.value == "local"


def test_build_embedding_provider_raises_on_unknown_backend() -> None:
    settings = build_settings(routing_embedding_backend="mystery")

    with pytest.raises(ValueError, match="Unsupported routing embedding backend"):
        build_embedding_provider(settings)


def test_build_embedding_provider_resolves_minilm_preset(monkeypatch: pytest.MonkeyPatch) -> None:
    created: dict[str, object] = {}

    class FakeProvider:
        def __init__(self, *, model_path: str, tokenizer_path: str, providers: list[str]) -> None:
            created["model_path"] = model_path
            created["tokenizer_path"] = tokenizer_path
            created["providers"] = providers

        def embed(self, text: str) -> list[float]:
            return [0.0]

    monkeypatch.setattr("app.routing.factory.OnnxTextEmbeddingProvider", FakeProvider)

    settings = build_settings(
        routing_embedding_model_id="optimum/all-MiniLM-L6-v2",
        routing_embedding_model_path="",
        routing_embedding_tokenizer_path="",
    )

    provider = build_embedding_provider(settings)

    assert isinstance(provider, FakeProvider)
    assert created["providers"] == ["CPUExecutionProvider"]
    assert str(created["model_path"]).endswith("/models/optimum-all-MiniLM-L6-v2/model.onnx")
    assert str(created["tokenizer_path"]).endswith("/models/optimum-all-MiniLM-L6-v2/tokenizer.json")


def test_repo_root_resolves_to_workspace_root() -> None:
    assert _repo_root().as_posix().endswith("/MaskProxy")


def test_build_route_store_resolves_minilm_preset_examples_path(monkeypatch: pytest.MonkeyPatch) -> None:
    captured: dict[str, object] = {}

    class StaticProvider:
        def embed(self, text: str) -> list[float]:
            return [0.0]

    def fake_store_builder(examples_path: str, embedding_provider: object) -> str:
        captured["examples_path"] = examples_path
        captured["embedding_provider"] = embedding_provider
        return "store"

    monkeypatch.setattr("app.routing.factory.build_in_memory_route_store", fake_store_builder)

    settings = build_settings(
        routing_embedding_model_id="all-MiniLM-L6-v2",
        routing_examples_path="",
    )

    store = build_route_store(settings, embedding_provider=StaticProvider())

    assert store == "store"
    assert str(captured["examples_path"]).endswith("/models/optimum-all-MiniLM-L6-v2/routes.json")


def test_build_route_store_raises_on_unknown_backend() -> None:
    settings = build_settings(routing_store_backend="mystery")

    class StaticProvider:
        def embed(self, text: str) -> list[float]:
            return [0.0]

    with pytest.raises(ValueError, match="Unsupported routing store backend"):
        build_route_store(settings, embedding_provider=StaticProvider())
