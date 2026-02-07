#!/bin/bash
# Download all-MiniLM-L6-v2 model for RPM Repository Vector Search

set -e

MODEL_DIR="models/all-MiniLM-L6-v2"
BASE_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main"

echo "Downloading all-MiniLM-L6-v2 model..."

# Create directory
mkdir -p "$MODEL_DIR"
cd "$MODEL_DIR"

# Download files
echo "Downloading config.json..."
curl -L -O "$BASE_URL/config.json"

echo "Downloading model.safetensors (this may take a while)..."
curl -L -O "$BASE_URL/model.safetensors"

echo "Downloading tokenizer.json..."
curl -L -O "$BASE_URL/tokenizer.json"

echo "Downloading tokenizer_config.json..."
curl -L -O "$BASE_URL/tokenizer_config.json"

cd ../..

echo ""
echo "✓ Model downloaded successfully to $MODEL_DIR"
echo ""
echo "You can now run:"
echo "  ./target/release/rpm_repo_search build-embeddings"
