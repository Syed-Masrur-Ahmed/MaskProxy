from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Protocol

from app.routing.semantic import EmbeddingProvider


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
    type_ids: list[int]


class TokenizerLike(Protocol):
    def encode(self, text: str) -> TokenizerEncoding:
        ...


class OnnxTextEmbeddingProvider:
    def __init__(
        self,
        *,
        model_path: str,
        tokenizer_path: str,
        providers: list[str] | None = None,
        normalize: bool = True,
        session: OnnxSession | None = None,
        tokenizer: TokenizerLike | None = None,
        session_factory: Any | None = None,
        tokenizer_loader: Any | None = None,
    ) -> None:
        if not model_path:
            raise ValueError("OnnxTextEmbeddingProvider requires a model_path.")
        if not tokenizer_path:
            raise ValueError("OnnxTextEmbeddingProvider requires a tokenizer_path.")

        self._providers = providers or ["CPUExecutionProvider"]
        self._normalize = normalize
        self._session = session or self._create_session(
            model_path=model_path,
            providers=self._providers,
            session_factory=session_factory,
        )
        self._tokenizer = tokenizer or self._load_tokenizer(
            tokenizer_path=tokenizer_path,
            tokenizer_loader=tokenizer_loader,
        )

    def embed(self, text: str) -> list[float]:
        if not text.strip():
            return []

        encoding = self._tokenizer.encode(text)
        model_inputs = self._build_model_inputs(encoding)
        outputs = self._session.run(None, model_inputs)
        if not outputs:
            return []

        sequence_output = self._to_nested_3d(outputs[0])
        if sequence_output:
            embedding = self._mean_pool(sequence_output[0], encoding.attention_mask)
        else:
            embedding_2d = self._to_nested_2d(outputs[0])
            if not embedding_2d:
                return []
            embedding = [float(value) for value in embedding_2d[0]]

        if self._normalize:
            embedding = self._l2_normalize(embedding)
        return embedding

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

    def _mean_pool(self, token_vectors: list[list[float]], attention_mask: list[int]) -> list[float]:
        active_count = 0
        pooled = [0.0 for _ in range(len(token_vectors[0]))]

        for index, token_vector in enumerate(token_vectors):
            if index >= len(attention_mask) or not attention_mask[index]:
                continue
            active_count += 1
            for dimension, value in enumerate(token_vector):
                pooled[dimension] += float(value)

        if active_count == 0:
            return pooled

        return [value / active_count for value in pooled]

    def _l2_normalize(self, vector: list[float]) -> list[float]:
        norm = sum(value * value for value in vector) ** 0.5
        if norm == 0.0:
            return vector
        return [value / norm for value in vector]

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
        except ImportError as exc:  # pragma: no cover
            raise RuntimeError(
                "tokenizers is required for the semantic routing embedding backend."
            ) from exc

        return Tokenizer.from_file(tokenizer_path)

    def _load_session_factory(self) -> Any:
        try:
            from onnxruntime import InferenceSession
        except ImportError as exc:  # pragma: no cover
            raise RuntimeError(
                "onnxruntime is required for the semantic routing embedding backend."
            ) from exc

        return InferenceSession

    def _make_int_tensor(self, values: list[int]) -> Any:
        try:
            import numpy as np
        except ImportError:
            return [list(values)]

        return np.asarray([values], dtype=np.int64)

    def _to_nested_3d(self, value: Any) -> list[list[list[float]]]:
        if hasattr(value, "tolist"):
            value = value.tolist()

        if (
            isinstance(value, list)
            and value
            and isinstance(value[0], list)
            and value[0]
            and isinstance(value[0][0], list)
        ):
            return value

        return []

    def _to_nested_2d(self, value: Any) -> list[list[float]]:
        if hasattr(value, "tolist"):
            value = value.tolist()

        if isinstance(value, list) and value and isinstance(value[0], list):
            return value

        return []


def load_json_file(path: str) -> Any:
    return json.loads(Path(path).read_text(encoding="utf-8"))
