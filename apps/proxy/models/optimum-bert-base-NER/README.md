# `optimum/bert-base-NER` Artifacts

Drop the local ONNX artifacts for the preset model here:

- `model.onnx`
- `tokenizer.json`
- `labels.json`

When these files exist, the proxy can load them with:

```env
NER_ENABLED=true
NER_BACKEND=onnx
NER_MODEL_ID=optimum/bert-base-NER
```

You can still override any individual path with:

- `NER_ONNX_MODEL_PATH`
- `NER_ONNX_TOKENIZER_PATH`
- `NER_ONNX_LABELS_PATH`
