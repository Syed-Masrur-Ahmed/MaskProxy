"""Download ONNX model artifacts for the proxy.

Runs as a Docker init container or standalone script.
Skips downloads if the files already exist.
"""

import os
import shutil
import sys
from pathlib import Path

MODEL_DIR = Path(os.environ.get("MODEL_DIR", "/models"))
CONFIG_DIR = Path(os.environ.get("CONFIG_DIR", "/config"))

MODELS = {
    "optimum-bert-base-NER": {
        "repo": "dslim/bert-base-NER",
        "files": ["model.onnx", "tokenizer.json", "config.json"],
    },
    "optimum-all-MiniLM-L6-v2": {
        "repo": "sentence-transformers/all-MiniLM-L6-v2",
        "files": ["model.onnx", "tokenizer.json"],
    },
}

# Project-specific config files (not on HuggingFace)
CONFIG_FILES = {
    "optimum-bert-base-NER": ["labels.json"],
    "optimum-all-MiniLM-L6-v2": ["routes.json"],
}


def download_models():
    from huggingface_hub import hf_hub_download

    for name, spec in MODELS.items():
        dest = MODEL_DIR / name
        dest.mkdir(parents=True, exist_ok=True)

        # Download model files from HuggingFace
        for filename in spec["files"]:
            target = dest / filename
            if target.exists():
                print(f"  [skip] {target} already exists")
                continue

            print(f"  [download] {spec['repo']}/{filename} -> {target}")
            downloaded = None
            for subfolder in ["onnx", None]:
                try:
                    path = hf_hub_download(
                        repo_id=spec["repo"],
                        filename=filename,
                        subfolder=subfolder,
                    )
                    # hf_hub_download returns a cache path; copy to our flat layout
                    shutil.copy2(path, target)
                    downloaded = True
                    break
                except Exception:
                    continue

            if downloaded is None:
                print(f"  [ERROR] Failed to download {filename} from {spec['repo']}")
                sys.exit(1)

        # Copy project-specific config files
        for cfg in CONFIG_FILES.get(name, []):
            src = CONFIG_DIR / name / cfg
            target = dest / cfg
            if target.exists():
                print(f"  [skip] {target} already exists")
                continue
            if src.exists():
                shutil.copy2(src, target)
                print(f"  [copy] {src} -> {target}")
            else:
                print(f"  [WARN] Config file not found: {src}")

    print("All models ready.")


if __name__ == "__main__":
    download_models()
