# Automated Model Downloads via Docker Compose

Model artifacts are now automatically downloaded when running `docker compose up`.

## How it works

A `model-downloader` init service runs before the proxy starts:

1. Downloads ONNX models from HuggingFace into a shared `model_data` volume
2. Copies project-specific config files (`labels.json`, `routes.json`) from the repo
3. Exits — the proxy starts with all models available at `/models/`

## Models downloaded

| Model | Source | Size |
|---|---|---|
| NER (bert-base-NER) | `dslim/bert-base-NER` | ~431 MB |
| Embeddings (all-MiniLM-L6-v2) | `sentence-transformers/all-MiniLM-L6-v2` | ~90 MB |

## Notes

- First run downloads ~520 MB total. Subsequent runs skip existing files.
- Models persist in the `model_data` Docker volume across restarts.
- To re-download, remove the volume: `docker volume rm maskproxy_model_data`
- The download script is at `apps/proxy/models/download.py`.
