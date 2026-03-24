from __future__ import annotations

from typing import Any

import pytest

from app.config import Settings
from app.detectors.factory import build_ner_backend
from app.detectors.ner import NerPrediction
from app.detectors.backends.huggingface import HuggingFaceTokenClassificationBackend
from app.detectors.backends.onnx import OnnxTokenClassificationBackend


def build_settings(**overrides: Any) -> Settings:
    values = {
        "redis_url": "redis://cache:6379/0",
        "upstream_base_url": "http://upstream.test",
        "mapping_ttl_seconds": 900,
        "max_body_bytes": 1_048_576,
        "ner_enabled": False,
        "ner_backend": "none",
        "ner_model_id": "",
        "ner_device": -1,
        "ner_onnx_model_path": "",
        "ner_onnx_tokenizer_path": "",
        "ner_onnx_labels_path": "",
        "ner_onnx_providers": "CPUExecutionProvider",
        "ner_confidence_threshold": 0.75,
    }
    values.update(overrides)
    return Settings(**values)


def test_huggingface_backend_converts_pipeline_output_to_predictions() -> None:
    def fake_pipeline_factory(**kwargs):
        assert kwargs["task"] == "token-classification"
        assert kwargs["model"] == "test-model"
        assert kwargs["aggregation_strategy"] == "simple"

        def fake_pipeline(text: str):
            assert text == "John Smith"
            return [
                {
                    "entity_group": "PER",
                    "start": 0,
                    "end": 10,
                    "score": 0.98,
                    "word": "John Smith",
                }
            ]

        return fake_pipeline

    backend = HuggingFaceTokenClassificationBackend(
        model_id="test-model",
        pipeline_factory=fake_pipeline_factory,
    )

    assert backend.predict("John Smith") == [
        NerPrediction(start=0, end=10, label="PER", score=0.98, text="John Smith")
    ]


def test_huggingface_backend_skips_malformed_pipeline_items() -> None:
    def fake_pipeline_factory(**kwargs):
        def fake_pipeline(text: str):
            return [
                {"entity_group": "PER", "start": 0, "end": 4, "score": 0.9},
                {"entity_group": "PER", "start": "bad", "end": 4, "score": 0.9},
                {"entity_group": "PER", "start": 0, "end": 4, "score": "bad"},
            ]

        return fake_pipeline

    backend = HuggingFaceTokenClassificationBackend(
        model_id="test-model",
        pipeline_factory=fake_pipeline_factory,
    )

    assert backend.predict("John") == [
        NerPrediction(start=0, end=4, label="PER", score=0.9, text="")
    ]


def test_build_ner_backend_returns_none_when_disabled() -> None:
    settings = build_settings()

    assert build_ner_backend(settings) is None


def test_build_ner_backend_raises_on_unknown_backend() -> None:
    settings = build_settings(ner_enabled=True, ner_backend="mystery")

    with pytest.raises(ValueError, match="Unsupported NER backend"):
        build_ner_backend(settings)


def test_build_ner_backend_constructs_huggingface_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    created: dict[str, Any] = {}

    class FakeBackend:
        def __init__(self, model_id: str, device: int, token: str | None) -> None:
            created["model_id"] = model_id
            created["device"] = device
            created["token"] = token

    monkeypatch.setattr("app.detectors.factory.HuggingFaceTokenClassificationBackend", FakeBackend)

    settings = build_settings(
        ner_enabled=True,
        ner_backend="huggingface",
        ner_model_id="dslim/bert-base-NER",
        ner_device=0,
        ner_hf_token="secret",
    )

    backend = build_ner_backend(settings)

    assert isinstance(backend, FakeBackend)
    assert created == {
        "model_id": "dslim/bert-base-NER",
        "device": 0,
        "token": "secret",
    }


def test_build_ner_backend_constructs_onnx_backend(monkeypatch: pytest.MonkeyPatch) -> None:
    created: dict[str, Any] = {}

    class FakeBackend:
        def __init__(
            self,
            model_path: str,
            tokenizer_path: str,
            labels_path: str,
            *,
            providers: list[str],
        ) -> None:
            created["model_path"] = model_path
            created["tokenizer_path"] = tokenizer_path
            created["labels_path"] = labels_path
            created["providers"] = providers

    monkeypatch.setattr("app.detectors.factory.OnnxTokenClassificationBackend", FakeBackend)

    settings = build_settings(
        ner_enabled=True,
        ner_backend="onnx",
        ner_onnx_model_path="/models/ner.onnx",
        ner_onnx_tokenizer_path="/models/tokenizer.json",
        ner_onnx_labels_path="/models/labels.json",
        ner_onnx_providers="CPUExecutionProvider,CUDAExecutionProvider",
    )

    backend = build_ner_backend(settings)

    assert isinstance(backend, FakeBackend)
    assert created == {
        "model_path": "/models/ner.onnx",
        "tokenizer_path": "/models/tokenizer.json",
        "labels_path": "/models/labels.json",
        "providers": ["CPUExecutionProvider", "CUDAExecutionProvider"],
    }


def test_build_ner_backend_resolves_optimum_bert_base_ner_preset(monkeypatch: pytest.MonkeyPatch) -> None:
    created: dict[str, Any] = {}

    class FakeBackend:
        def __init__(
            self,
            model_path: str,
            tokenizer_path: str,
            labels_path: str,
            *,
            providers: list[str],
        ) -> None:
            created["model_path"] = model_path
            created["tokenizer_path"] = tokenizer_path
            created["labels_path"] = labels_path
            created["providers"] = providers

    monkeypatch.setattr("app.detectors.factory.OnnxTokenClassificationBackend", FakeBackend)

    settings = build_settings(
        ner_enabled=True,
        ner_backend="onnx",
        ner_model_id="optimum/bert-base-NER",
    )

    backend = build_ner_backend(settings)

    assert isinstance(backend, FakeBackend)
    assert created["model_path"].endswith("/apps/proxy/models/optimum-bert-base-NER/model.onnx")
    assert created["tokenizer_path"].endswith("/apps/proxy/models/optimum-bert-base-NER/tokenizer.json")
    assert created["labels_path"].endswith("/apps/proxy/models/optimum-bert-base-NER/labels.json")
    assert created["providers"] == ["CPUExecutionProvider"]
