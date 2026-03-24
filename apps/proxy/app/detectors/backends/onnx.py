from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any, Protocol

from app.detectors.ner import NerPrediction


class SessionInput(Protocol):
    name: str


class OnnxSession(Protocol):
    def get_inputs(self) -> list[SessionInput]:
        ...

    def run(self, output_names: list[str] | None, inputs: dict[str, Any]) -> list[Any]:
        ...


class TokenizerEncoding(Protocol):
    ids: list[int]
    attention_mask: list[int]
    offsets: list[tuple[int, int]]
    type_ids: list[int]


class TokenizerLike(Protocol):
    def encode(self, text: str) -> TokenizerEncoding:
        ...


@dataclass(frozen=True)
class ParsedEntityLabel:
    prefix: str
    kind: str


class OnnxTokenClassificationBackend:
    def __init__(
        self,
        model_path: str,
        tokenizer_path: str,
        labels_path: str,
        *,
        providers: list[str] | None = None,
        session: OnnxSession | None = None,
        tokenizer: TokenizerLike | None = None,
        session_factory: Any | None = None,
        tokenizer_loader: Any | None = None,
        label_loader: Any | None = None,
    ) -> None:
        if not model_path:
            raise ValueError("OnnxTokenClassificationBackend requires a model_path.")
        if not tokenizer_path:
            raise ValueError("OnnxTokenClassificationBackend requires a tokenizer_path.")
        if not labels_path:
            raise ValueError("OnnxTokenClassificationBackend requires a labels_path.")

        resolved_providers = providers or ["CPUExecutionProvider"]
        resolved_label_loader = label_loader or self._load_labels
        self._session = session or self._create_session(
            model_path=model_path,
            providers=resolved_providers,
            session_factory=session_factory,
        )
        self._tokenizer = tokenizer or self._load_tokenizer(
            tokenizer_path=tokenizer_path,
            tokenizer_loader=tokenizer_loader,
        )
        self._id_to_label = resolved_label_loader(labels_path)

    def predict(self, text: str) -> list[NerPrediction]:
        if not text:
            return []

        encoding = self._tokenizer.encode(text)
        model_inputs = self._build_model_inputs(encoding)
        raw_outputs = self._session.run(None, model_inputs)
        if not raw_outputs:
            return []

        logits = self._to_nested_3d(raw_outputs[0])
        if not logits or not logits[0] or not logits[0][0]:
            return []

        token_scores = self._softmax(logits[0])
        label_indices = [self._argmax(row) for row in token_scores]

        predictions: list[NerPrediction] = []
        current_start: int | None = None
        current_end: int | None = None
        current_kind: str | None = None
        current_scores: list[float] = []

        for token_index, label_index in enumerate(label_indices):
            label = self._id_to_label.get(int(label_index))
            offsets = self._offset_at(encoding, token_index)
            if label is None or offsets is None:
                if current_kind is not None:
                    predictions.append(
                        self._build_prediction(
                            text,
                            current_start,
                            current_end,
                            current_kind,
                            current_scores,
                        )
                    )
                    current_start = current_end = None
                    current_kind = None
                    current_scores = []
                continue

            start, end = offsets
            if start == end:
                continue

            parsed = self._parse_label(label)
            if parsed.kind == "O":
                if current_kind is not None:
                    predictions.append(
                        self._build_prediction(
                            text,
                            current_start,
                            current_end,
                            current_kind,
                            current_scores,
                        )
                    )
                    current_start = current_end = None
                    current_kind = None
                    current_scores = []
                continue

            score = float(token_scores[token_index][int(label_index)])
            if current_kind is None:
                current_start = start
                current_end = end
                current_kind = parsed.kind
                current_scores = [score]
                continue

            if self._should_extend_entity(parsed, current_kind, start, current_end):
                current_end = max(current_end or end, end)
                current_scores.append(score)
                continue

            predictions.append(
                self._build_prediction(
                    text,
                    current_start,
                    current_end,
                    current_kind,
                    current_scores,
                )
            )
            current_start = start
            current_end = end
            current_kind = parsed.kind
            current_scores = [score]

        if current_kind is not None:
            predictions.append(
                self._build_prediction(
                    text,
                    current_start,
                    current_end,
                    current_kind,
                    current_scores,
                )
            )

        return predictions

    def _build_model_inputs(self, encoding: TokenizerEncoding) -> dict[str, Any]:
        session_input_names = {session_input.name for session_input in self._session.get_inputs()}
        model_inputs: dict[str, Any] = {}

        if "input_ids" in session_input_names:
            model_inputs["input_ids"] = self._make_int_tensor(encoding.ids)
        if "attention_mask" in session_input_names:
            model_inputs["attention_mask"] = self._make_int_tensor(encoding.attention_mask)
        type_ids = getattr(encoding, "type_ids", None)
        if "token_type_ids" in session_input_names and type_ids is not None:
            model_inputs["token_type_ids"] = self._make_int_tensor(type_ids)

        return model_inputs

    def _create_session(
        self,
        *,
        model_path: str,
        providers: list[str],
        session_factory: Any | None,
    ) -> OnnxSession:
        factory = session_factory or self._load_session_factory()
        return factory(model_path, providers=providers)

    def _load_tokenizer(
        self,
        *,
        tokenizer_path: str,
        tokenizer_loader: Any | None,
    ) -> TokenizerLike:
        if tokenizer_loader is not None:
            return tokenizer_loader(tokenizer_path)

        try:
            from tokenizers import Tokenizer
        except ImportError as exc:  # pragma: no cover - exercised only without dependency
            raise RuntimeError(
                "tokenizers is required for the ONNX NER backend. "
                "Install it before enabling NER_BACKEND=onnx."
            ) from exc

        return Tokenizer.from_file(tokenizer_path)

    def _load_session_factory(self) -> Any:
        try:
            from onnxruntime import InferenceSession
        except ImportError as exc:  # pragma: no cover - exercised only without dependency
            raise RuntimeError(
                "onnxruntime is required for the ONNX NER backend. "
                "Install it before enabling NER_BACKEND=onnx."
            ) from exc

        return InferenceSession

    def _load_labels(self, labels_path: str) -> dict[int, str]:
        raw = json.loads(Path(labels_path).read_text(encoding="utf-8"))
        if isinstance(raw, list):
            return {
                index: label
                for index, label in enumerate(raw)
                if isinstance(label, str)
            }

        if isinstance(raw, dict):
            if all(isinstance(key, str) and key.isdigit() for key in raw):
                return {
                    int(key): value
                    for key, value in raw.items()
                    if isinstance(value, str)
                }
            if all(isinstance(value, int) for value in raw.values()):
                return {
                    value: key
                    for key, value in raw.items()
                    if isinstance(key, str)
                }

        raise ValueError("Unsupported ONNX label map format.")

    def _softmax(self, logits: list[list[float]]) -> list[list[float]]:
        normalized_rows: list[list[float]] = []
        for row in logits:
            max_value = max(row)
            exps = [pow(2.718281828459045, value - max_value) for value in row]
            total = sum(exps)
            normalized_rows.append([value / total for value in exps])
        return normalized_rows

    def _argmax(self, row: list[float]) -> int:
        best_index = 0
        best_value = row[0]
        for index, value in enumerate(row[1:], start=1):
            if value > best_value:
                best_index = index
                best_value = value
        return best_index

    def _offset_at(self, encoding: TokenizerEncoding, token_index: int) -> tuple[int, int] | None:
        if token_index >= len(encoding.offsets):
            return None
        start, end = encoding.offsets[token_index]
        if not isinstance(start, int) or not isinstance(end, int):
            return None
        return start, end

    def _parse_label(self, label: str) -> ParsedEntityLabel:
        normalized = label.upper()
        if "-" in normalized:
            prefix, _, suffix = normalized.partition("-")
            if prefix in {"B", "I", "L", "U", "E", "S"}:
                return ParsedEntityLabel(prefix=prefix, kind=suffix)
        return ParsedEntityLabel(prefix="", kind=normalized)

    def _should_extend_entity(
        self,
        parsed: ParsedEntityLabel,
        current_kind: str,
        start: int,
        current_end: int | None,
    ) -> bool:
        if parsed.kind != current_kind or current_end is None:
            return False

        if parsed.prefix in {"I", "L", "E"}:
            return start >= current_end

        if parsed.prefix == "B":
            return start >= current_end and (start - current_end) <= 1

        if parsed.prefix == "":
            return start >= current_end and (start - current_end) <= 1

        return False

    def _build_prediction(
        self,
        text: str,
        start: int | None,
        end: int | None,
        kind: str,
        scores: list[float],
    ) -> NerPrediction:
        if start is None or end is None:
            raise ValueError("Cannot build NER prediction without valid offsets.")

        return NerPrediction(
            start=start,
            end=end,
            label=kind,
            score=sum(scores) / len(scores),
            text=text[start:end],
        )

    def _make_int_tensor(self, values: list[int]) -> Any:
        try:
            import numpy as np
        except ImportError:
            return [list(values)]

        return np.asarray([values], dtype=np.int64)

    def _to_nested_3d(self, value: Any) -> list[list[list[float]]]:
        if hasattr(value, "tolist"):
            value = value.tolist()

        if not isinstance(value, list):
            return []
        if not value or not isinstance(value[0], list):
            return []
        if not value[0] or not isinstance(value[0][0], list):
            return []

        return value
