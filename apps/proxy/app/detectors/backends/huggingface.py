from __future__ import annotations

from collections.abc import Callable
from typing import Any

from app.detectors.ner import NerPrediction

PipelineFactory = Callable[..., Any]


class HuggingFaceTokenClassificationBackend:
    def __init__(
        self,
        model_id: str,
        device: int = -1,
        aggregation_strategy: str = "simple",
        token: str | None = None,
        pipeline_factory: PipelineFactory | None = None,
    ) -> None:
        if not model_id:
            raise ValueError("HuggingFaceTokenClassificationBackend requires a model_id.")

        factory = pipeline_factory or self._load_pipeline_factory()
        self._pipeline = factory(
            task="token-classification",
            model=model_id,
            tokenizer=model_id,
            aggregation_strategy=aggregation_strategy,
            device=device,
            token=token,
        )

    def predict(self, text: str) -> list[NerPrediction]:
        predictions: list[NerPrediction] = []
        for item in self._pipeline(text):
            label = item.get("entity_group") or item.get("entity")
            start = item.get("start")
            end = item.get("end")
            score = item.get("score")

            if (
                not isinstance(label, str)
                or not isinstance(start, int)
                or not isinstance(end, int)
                or not isinstance(score, (int, float))
            ):
                continue

            predictions.append(
                NerPrediction(
                    start=start,
                    end=end,
                    label=label,
                    score=float(score),
                    text=item.get("word", ""),
                )
            )

        return predictions

    def _load_pipeline_factory(self) -> PipelineFactory:
        try:
            from transformers import pipeline
        except ImportError as exc:  # pragma: no cover - exercised when dependency is absent
            raise RuntimeError(
                "Transformers is required for the Hugging Face NER backend. "
                "Install it before enabling NER_BACKEND=huggingface."
            ) from exc

        return pipeline

