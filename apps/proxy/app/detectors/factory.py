from __future__ import annotations

from pathlib import Path

from app.config import Settings
from app.detectors.backends import (
    HuggingFaceTokenClassificationBackend,
    OnnxTokenClassificationBackend,
)
from app.detectors.base import Detector
from app.detectors.composite import CompositeDetector
from app.detectors.ner import NerBackend, NerDetector
from app.detectors.regex import RegexDetector

ONNX_PRESET_DIRS: dict[str, str] = {
    "bert-base-NER": "optimum-bert-base-NER",
    "optimum/bert-base-NER": "optimum-bert-base-NER",
}


def _proxy_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _resolve_onnx_artifact_paths(settings: Settings) -> tuple[str, str, str]:
    preset_dir_name = ONNX_PRESET_DIRS.get(settings.ner_model_id)
    preset_dir = _proxy_root() / "models" / preset_dir_name if preset_dir_name else None

    model_path = settings.ner_onnx_model_path
    tokenizer_path = settings.ner_onnx_tokenizer_path
    labels_path = settings.ner_onnx_labels_path

    if preset_dir is not None:
        model_path = model_path or str(preset_dir / "model.onnx")
        tokenizer_path = tokenizer_path or str(preset_dir / "tokenizer.json")
        labels_path = labels_path or str(preset_dir / "labels.json")

    return model_path, tokenizer_path, labels_path


def build_ner_backend(settings: Settings) -> NerBackend | None:
    if not settings.ner_enabled:
        return None

    if settings.ner_backend == "onnx":
        model_path, tokenizer_path, labels_path = _resolve_onnx_artifact_paths(settings)
        providers = [
            provider.strip()
            for provider in settings.ner_onnx_providers.split(",")
            if provider.strip()
        ] or ["CPUExecutionProvider"]
        return OnnxTokenClassificationBackend(
            model_path=model_path,
            tokenizer_path=tokenizer_path,
            labels_path=labels_path,
            providers=providers,
        )

    if settings.ner_backend == "huggingface":
        return HuggingFaceTokenClassificationBackend(
            model_id=settings.ner_model_id,
            device=settings.ner_device,
            token=settings.ner_hf_token,
        )

    if settings.ner_backend in {"", "none"}:
        return None

    raise ValueError(f"Unsupported NER backend: {settings.ner_backend}")


def build_runtime_detector(settings: Settings, ner_backend: NerBackend | None = None) -> Detector:
    detectors: list[Detector] = [RegexDetector()]

    resolved_ner_backend = ner_backend or build_ner_backend(settings)

    if settings.ner_enabled and resolved_ner_backend is not None:
        detectors.append(
            NerDetector(
                backend=resolved_ner_backend,
                confidence_threshold=settings.ner_confidence_threshold,
            )
        )

    return CompositeDetector(detectors)
