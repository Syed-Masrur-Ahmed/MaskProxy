from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

from app.detectors.backends.onnx import OnnxTokenClassificationBackend


@dataclass
class FakeEncoding:
    ids: list[int]
    attention_mask: list[int]
    offsets: list[tuple[int, int]]
    type_ids: list[int]


class FakeTokenizer:
    def __init__(self, encoding: FakeEncoding) -> None:
        self._encoding = encoding

    def encode(self, text: str) -> FakeEncoding:
        return self._encoding


@dataclass
class FakeInput:
    name: str


class FakeSession:
    def __init__(self, outputs: list[Any], input_names: list[str] | None = None) -> None:
        self._outputs = outputs
        self._input_names = input_names or ["input_ids", "attention_mask", "token_type_ids"]
        self.last_inputs: dict[str, Any] | None = None

    def get_inputs(self) -> list[FakeInput]:
        return [FakeInput(name=name) for name in self._input_names]

    def run(self, output_names: list[str] | None, inputs: dict[str, Any]) -> list[Any]:
        self.last_inputs = inputs
        return self._outputs


def test_onnx_backend_merges_bio_tokens_into_entity_predictions() -> None:
    text = "John Smith works"
    encoding = FakeEncoding(
        ids=[101, 1001, 1002, 1003, 102],
        attention_mask=[1, 1, 1, 1, 1],
        offsets=[(0, 0), (0, 4), (5, 10), (11, 16), (0, 0)],
        type_ids=[0, 0, 0, 0, 0],
    )
    session = FakeSession(
        [
            [
                [
                    [9.0, 1.0, 1.0, 1.0],
                    [1.0, 9.0, 1.0, 1.0],
                    [1.0, 1.0, 9.0, 1.0],
                    [9.0, 1.0, 1.0, 1.0],
                    [9.0, 1.0, 1.0, 1.0],
                ]
            ]
        ]
    )

    backend = OnnxTokenClassificationBackend(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        labels_path="labels.json",
        session=session,
        tokenizer=FakeTokenizer(encoding),
        label_loader=lambda _: {0: "O", 1: "B-PER", 2: "I-PER", 3: "B-ORG"},
    )

    predictions = backend.predict(text)

    assert len(predictions) == 1
    assert predictions[0].start == 0
    assert predictions[0].end == 10
    assert predictions[0].label == "PER"
    assert predictions[0].text == "John Smith"
    assert predictions[0].score > 0.99
    assert session.last_inputs is not None
    assert set(session.last_inputs) == {"input_ids", "attention_mask", "token_type_ids"}


def test_onnx_backend_supports_label_to_index_maps(tmp_path: Path) -> None:
    labels_path = tmp_path / "labels.json"
    labels_path.write_text('{"O": 0, "B-PER": 1}', encoding="utf-8")

    backend = OnnxTokenClassificationBackend(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        labels_path=str(labels_path),
        session=FakeSession([[[0.0, 0.0]]]),
        tokenizer=FakeTokenizer(
            FakeEncoding(ids=[1], attention_mask=[1], offsets=[(0, 0)], type_ids=[0])
        ),
    )

    assert backend._id_to_label == {0: "O", 1: "B-PER"}


def test_onnx_backend_merges_adjacent_b_tags_for_split_name_tokens() -> None:
    text = "Hiroshi Tanaka"
    encoding = FakeEncoding(
        ids=[101, 1001, 1002, 1003, 1004, 102],
        attention_mask=[1, 1, 1, 1, 1, 1],
        offsets=[(0, 0), (0, 2), (2, 7), (8, 11), (11, 14), (0, 0)],
        type_ids=[0, 0, 0, 0, 0, 0],
    )
    session = FakeSession(
        [
            [
                [
                    [9.0, 1.0],
                    [1.0, 9.0],
                    [1.0, 9.0],
                    [1.0, 9.0],
                    [1.0, 9.0],
                    [9.0, 1.0],
                ]
            ]
        ]
    )

    backend = OnnxTokenClassificationBackend(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        labels_path="labels.json",
        session=session,
        tokenizer=FakeTokenizer(encoding),
        label_loader=lambda _: {0: "O", 1: "B-PER"},
    )

    predictions = backend.predict(text)

    assert len(predictions) == 1
    assert predictions[0].start == 0
    assert predictions[0].end == len(text)
    assert predictions[0].label == "PER"
    assert predictions[0].text == "Hiroshi Tanaka"


def test_onnx_backend_skips_empty_text_without_invoking_session() -> None:
    session = FakeSession([[[0.0, 0.0]]])
    backend = OnnxTokenClassificationBackend(
        model_path="model.onnx",
        tokenizer_path="tokenizer.json",
        labels_path="labels.json",
        session=session,
        tokenizer=FakeTokenizer(
            FakeEncoding(ids=[1], attention_mask=[1], offsets=[(0, 0)], type_ids=[0])
        ),
        label_loader=lambda _: {0: "O", 1: "B-PER"},
    )

    assert backend.predict("") == []
    assert session.last_inputs is None


def test_onnx_backend_requires_artifact_paths() -> None:
    with pytest.raises(ValueError, match="model_path"):
        OnnxTokenClassificationBackend(
            model_path="",
            tokenizer_path="tokenizer.json",
            labels_path="labels.json",
            session=FakeSession([]),
            tokenizer=FakeTokenizer(
                FakeEncoding(ids=[1], attention_mask=[1], offsets=[(0, 0)], type_ids=[0])
            ),
            label_loader=lambda _: {0: "O"},
        )
