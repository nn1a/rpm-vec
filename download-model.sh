#!/bin/bash
# Download embedding models for RPM Repository Vector Search
#
# Usage:
#   ./download-model.sh           # Download default (all-MiniLM-L6-v2)
#   ./download-model.sh minilm    # Download all-MiniLM-L6-v2 (English)
#   ./download-model.sh e5        # Download multilingual-e5-small (100 languages)
#   ./download-model.sh all       # Download both models

set -e

download_minilm() {
    local MODEL_DIR="models/all-MiniLM-L6-v2"
    local BASE_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main"

    echo "Downloading all-MiniLM-L6-v2 model (English, ~80MB)..."

    mkdir -p "$MODEL_DIR"
    cd "$MODEL_DIR"

    echo "  config.json..."
    curl -sL -O "$BASE_URL/config.json"

    echo "  model.safetensors..."
    curl -L -O "$BASE_URL/model.safetensors"

    echo "  tokenizer.json..."
    curl -sL -O "$BASE_URL/tokenizer.json"

    echo "  tokenizer_config.json..."
    curl -sL -O "$BASE_URL/tokenizer_config.json"

    cd ../..

    echo "✓ all-MiniLM-L6-v2 downloaded to $MODEL_DIR"
    echo ""
}

download_e5() {
    local MODEL_DIR="models/multilingual-e5-small"
    local BASE_URL="https://huggingface.co/intfloat/multilingual-e5-small/resolve/main"

    echo "Downloading multilingual-e5-small model (100 languages, ~470MB)..."

    mkdir -p "$MODEL_DIR"
    cd "$MODEL_DIR"

    echo "  config.json..."
    curl -sL -O "$BASE_URL/config.json"

    echo "  model.safetensors..."
    curl -L -O "$BASE_URL/model.safetensors"

    echo "  tokenizer.json..."
    curl -sL -O "$BASE_URL/tokenizer.json"

    echo "  tokenizer_config.json..."
    curl -sL -O "$BASE_URL/tokenizer_config.json"

    cd ../..

    echo "✓ multilingual-e5-small downloaded to $MODEL_DIR"
    echo ""
}

MODEL_TYPE="${1:-minilm}"

case "$MODEL_TYPE" in
    minilm)
        download_minilm
        echo "Usage: rpm_repo_search index embeddings"
        ;;
    e5)
        download_e5
        echo "Usage: rpm_repo_search index embeddings --model-type e5-multilingual"
        ;;
    all)
        download_minilm
        download_e5
        echo "Usage:"
        echo "  rpm_repo_search index embeddings                          # English (MiniLM)"
        echo "  rpm_repo_search index embeddings --model-type e5-multilingual  # Multilingual (E5)"
        ;;
    *)
        echo "Unknown model type: $MODEL_TYPE"
        echo "Usage: $0 [minilm|e5|all]"
        exit 1
        ;;
esac
