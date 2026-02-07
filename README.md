# RPM Repository Vector Search

Rust-based RPM repository metadata indexing and semantic search tool.

## Features

- 🔍 **Semantic Search**: Natural language queries for RPM packages
- 📊 **Metadata Storage**: SQLite-based structured storage
- 🎯 **Vector Indexing**: Efficient similarity search using sqlite-vec (statically linked)
- 🚀 **Local/Offline**: Fully operational without internet
- 📦 **rpm-md Support**: Parses standard RPM repository metadata
- 🗜️ **Multiple Compression**: Supports gzip (.gz) and zstandard (.zst)
- 🤖 **MCP Server**: Model Context Protocol support for AI agents (Claude Desktop, etc.)
- 🔄 **Auto Sync**: Automatic repository synchronization with scheduling

## Architecture

```
rpm-md XML → Parser → Normalizer → SQLite + Vector Store → Search API
```

### Technology Stack

- **Language**: Rust
- **Embedding**: Candle + all-MiniLM-L6-v2 (384 dimensions)
- **Vector Store**: SQLite with custom vector operations
- **Metadata Store**: SQLite

## Installation

### Prerequisites

- Rust 1.70+
- SQLite 3.x

### Build from Source

```bash
git clone <repository-url>
cd rpm-vec

# Standard build (CPU-based embeddings)
cargo build --release

# Build with Apple Accelerate (macOS - recommended for Apple Silicon)
cargo build --release --features accelerate

# Build with NVIDIA GPU acceleration (Linux with CUDA)
cargo build --release --features cuda

# Build with all features (Accelerate + MCP - recommended for macOS)
cargo build --release --features "accelerate,mcp"
```

The compiled binary will be at `target/release/rpm_repo_search`.

**Build Options:**
- **Default**: Includes embedding support (CPU-only), vector indexing, and automatic repository synchronization with sqlite-vec static linking
- **`accelerate`**: Enable Apple Accelerate framework optimization (recommended for macOS)
  - Optimized BLAS/LAPACK operations on Apple hardware
  - Significantly faster than plain CPU on macOS
- **`cuda`**: Enable CUDA GPU acceleration for NVIDIA GPUs
  - Automatically falls back to CPU if GPU unavailable
  - Requires CUDA toolkit installed
- **`mcp`**: Add Model Context Protocol server support

## Usage

### 1. Index a Repository

First, download the `primary.xml.gz` (or `primary.xml.zst`) file from an RPM repository:

```bash
# Example: Rocky Linux 9
wget https://download.rockylinux.org/pub/rocky/9/BaseOS/x86_64/os/repodata/primary.xml.gz

# Index it (supports .gz and .zst compression)
./rpm_repo_search index --file primary.xml.gz --repo rocky9-baseos
```

### 2. Build Embeddings

Download the MiniLM model (one-time setup):

**Option 1: Use the download script (recommended)**

```bash
./download-model.sh
```

**Option 2: Manual download**

```bash
# Create model directory
mkdir -p models/all-MiniLM-L6-v2
cd models/all-MiniLM-L6-v2

# Download model files from HuggingFace
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors
wget https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json

cd ../..
```

**Build embeddings:**

```bash
# Incremental (default) - only builds for new packages
./rpm_repo_search build-embeddings

# Full rebuild - drop all embeddings and regenerate
./rpm_repo_search build-embeddings --rebuild

# With detailed batch information
./rpm_repo_search build-embeddings --verbose
```

### 3. Optional: SQLite-vec Extension (for Large Datasets)

For repositories with 100K+ packages, you can optionally install the [sqlite-vec](https://github.com/asg017/sqlite-vec) extension for faster vector search.

**Installation:**

```bash
# Linux/macOS - Download pre-built extension
wget https://github.com/asg017/sqlite-vec/releases/download/v0.1.0/sqlite-vec-[OS]-[ARCH].so
mv sqlite-vec-*.so ~/.local/lib/vec0.so

# Or build from source
git clone https://github.com/asg017/sqlite-vec.git
cd sqlite-vec
make
# Copy vec0.so to a known location
```

**Configuration:**

Modify your config to point to the extension:
```rust
Config {
    sqlite_vec_path: Some("/path/to/vec0.so".into()),
    // ... other settings
}
```

**Note:** The tool automatically falls back to manual cosine similarity if the extension is not available. For most use cases (<10K packages), the fallback is sufficient.

For bundling options, see [docs/SQLITE_VEC_BUNDLING.md](docs/SQLITE_VEC_BUNDLING.md).

### 3. Performance Tips

**Hardware Acceleration (Recommended for macOS):**

If you're on macOS (especially Apple Silicon), rebuild with the `accelerate` feature for significantly faster embedding generation:

```bash
# Rebuild with Apple Accelerate framework
cargo build --release --features "embedding,accelerate"

# Or add to default build
cargo build --release --features "accelerate"
```

This uses Apple's optimized BLAS/LAPACK implementation and can speed up embeddings 2-5x on Apple Silicon.

**For Linux with NVIDIA GPU:**

```bash
# Requires CUDA toolkit installed
cargo build --release --features "embedding,cuda"
```

**Build time comparison (19K packages):**
- CPU only: ~3-5 minutes
- With accelerate (macOS): ~1-2 minutes  
- With CUDA (NVIDIA): ~30-60 seconds

### 4. Search Packages

**Natural language search:**
```bash
./rpm_repo_search search "cryptography library for SSL"
```

**With filters:**
```bash
# Search for network packages that don't require glibc >= 2.34
./rpm_repo_search search "network tools" --not-requiring glibc

# Search for x86_64 packages providing libssl
./rpm_repo_search search "ssl library" --arch x86_64 --providing libssl.so.3
```

**View statistics:**
```bash
./rpm_repo_search stats
```

## Commands

### `index`
Index RPM repository metadata from primary.xml file.

**Options:**
- `-f, --file <PATH>`: Path to primary.xml, primary.xml.gz, or primary.xml.zst
- `-r, --repo <NAME>`: Repository name
- `-u, --update`: Update existing repository (incremental update)

**Examples:**
```bash
# Initial indexing
./rpm_repo_search index -f primary.xml.gz -r rocky9-baseos

# Incremental update (add new, update changed, remove deleted packages)
./rpm_repo_search index -f primary-updated.xml.gz -r rocky9-baseos --update
```

### `build-embeddings`
Generate vector embeddings for indexed packages.

By default, runs incrementally — only generates embeddings for packages that don't have one yet. Use `--rebuild` to force a full rebuild.

**Options:**
- `-m, --model <PATH>`: Model directory (default: models/all-MiniLM-L6-v2)
- `-t, --tokenizer <PATH>`: Tokenizer file (default: models/all-MiniLM-L6-v2/tokenizer.json)
- `-v, --verbose`: Show detailed batch information (progress is always shown)
- `--rebuild`: Force full rebuild (drop all embeddings and regenerate)

**Examples:**

```bash
# Incremental (default) - fast, only processes new packages
./rpm_repo_search build-embeddings

# Full rebuild - useful after model changes
./rpm_repo_search build-embeddings --rebuild

# With logging to see which device is being used
RUST_LOG=info ./rpm_repo_search build-embeddings
```

**Device selection (logged at INFO level):**
- `🚀 Using CUDA GPU for embeddings` - NVIDIA GPU detected
- `💻 Using CPU with Apple Accelerate framework` - Accelerate enabled (macOS)
- `💻 Using CPU for embeddings` - Plain CPU (no acceleration)

### `search`
Search for packages using natural language or filters.

**Arguments:**
- `QUERY`: Search query text

**Options:**
- `-a, --arch <ARCH>`: Filter by architecture
- `-r, --repo <REPO>`: Filter by repository
- `--not-requiring <DEP>`: Exclude packages requiring dependency
- `--providing <CAP>`: Include only packages providing capability
- `-n, --top-k <N>`: Number of results (default: 10)

### `stats`
Show database statistics.

### `list-repos`
List all indexed repositories with package counts.

### `repo-stats`
Show statistics for a specific repository.

**Arguments:**
- `REPO`: Repository name

### `delete-repo`
Delete a repository and all its packages.

**Arguments:**
- `REPO`: Repository name

**Options:**
- `-y, --yes`: Confirm deletion (required for safety)

## Multiple Repository Management

You can index and manage multiple repositories simultaneously:

```bash
# Index multiple repositories
./rpm_repo_search index -f rocky9-baseos.xml.gz -r rocky9-baseos
./rpm_repo_search index -f rocky9-appstream.xml.gz -r rocky9-appstream
./rpm_repo_search index -f fedora-39.xml.zst -r fedora-39

# List all repositories
./rpm_repo_search list-repos

# Repository statistics
./rpm_repo_search repo-stats rocky9-baseos

# Search in specific repository
./rpm_repo_search search "kernel" --repo rocky9-baseos

# Delete a repository
./rpm_repo_search delete-repo fedora-39 --yes
```

## Incremental Updates

Instead of re-indexing an entire repository, you can perform incremental updates to add new packages, update changed packages, and remove deleted packages:

```bash
# Initial indexing
./rpm_repo_search index -f rocky9-baseos.xml.gz -r rocky9-baseos

# Later, when the repository is updated
./rpm_repo_search index -f rocky9-baseos-updated.xml.gz -r rocky9-baseos --update

# The update will:
# - Add new packages that didn't exist before
# - Update packages with version changes
# - Remove packages no longer in the repository
```

**Update statistics are logged:**
```
INFO: Starting incremental update
INFO: Incremental update completed added=15 updated=42 removed=3 total=57
```

**Benefits:**
- **Fast**: Only processes changed packages
- **Efficient**: No need to rebuild embeddings for unchanged packages
- **Safe**: Transactional updates ensure consistency

## Repository Auto-Sync (Scheduling)

Automatically keep your local database synchronized with remote RPM repositories.

### Building with Sync Support

```bash
cargo build --release --features sync
```

### Quick Start

```bash
# 1. Generate example configuration
./rpm_repo_search sync-init

# 2. Edit configuration file
vim sync-config.toml

# 3. One-time sync
./rpm_repo_search sync-once

# 4. Run as daemon
./rpm_repo_search sync-daemon &

# 5. Check status
./rpm_repo_search sync-status
```

### Configuration Example

```toml
work_dir = ".rpm-sync"

[[repositories]]
name = "rocky9-baseos"
base_url = "https://dl.rockylinux.org/pub/rocky/9/BaseOS/x86_64/os"
interval_seconds = 3600  # Check every hour
enabled = true
arch = "x86_64"

[[repositories]]
name = "rocky9-appstream"
base_url = "https://dl.rockylinux.org/pub/rocky/9/AppStream/x86_64/os"
interval_seconds = 7200  # Check every 2 hours
enabled = true
arch = "x86_64"
```

### How It Works

1. **Fetch repomd.xml**: Downloads metadata index from repository
2. **Change Detection**: Compares checksum with previous sync
3. **Download primary.xml**: If changed, downloads package metadata
4. **Incremental Update**: Uses `--update` mode to sync database
5. **State Tracking**: Records sync status and timestamp

### Commands

- `sync-init`: Generate example configuration
- `sync-once`: One-time sync of all repositories
- `sync-daemon`: Continuous background syncing
- `sync-status`: Show sync status for all repositories

See [docs/SYNC_GUIDE.md](docs/SYNC_GUIDE.md) for complete guide.

## MCP Server (AI Agent Integration)

The MCP (Model Context Protocol) server allows AI agents like Claude Desktop to directly query your RPM package database.

### Building with MCP Support

```bash
cargo build --release --features mcp
```

### Claude Desktop Setup

Add to `~/.config/claude/config.json`:

```json
{
  "mcpServers": {
    "rpm-search": {
      "command": "/path/to/rpm_repo_search",
      "args": ["mcp-server"]
    }
  }
}
```

### Available Tools

The MCP server provides 5 tools:

1. **search_packages** - Search for RPM packages
2. **get_package_info** - Get detailed package information
3. **list_repositories** - List all indexed repositories
4. **compare_versions** - Compare RPM versions
5. **get_repository_stats** - Repository statistics

### Usage Example

In Claude Desktop:
```
You: "Find all kernel packages for x86_64 in Rocky Linux 9"
Claude: [calls search_packages with appropriate filters]

You: "Compare version 1.2.3-1 with 1.2.4-1"
Claude: [calls compare_versions and explains which is newer]
```

See [docs/MCP_GUIDE.md](docs/MCP_GUIDE.md) for complete integration guide.

## Project Structure

```
rpm-vec/
├── src/              # Source code
│   ├── main.rs       # CLI entry point
│   ├── repomd/       # RPM metadata parsing
│   ├── normalize/    # Data normalization
│   ├── storage/      # SQLite storage
│   ├── embedding/    # Vector embeddings
│   ├── search/       # Search engine
│   └── api/          # Public API
├── tests/            # Integration tests
├── docs/             # Documentation
│   ├── design/       # Design documents
│   └── *.md          # User & dev guides
├── Cargo.toml        # Project configuration
├── README.md         # This file
├── AGENTS.md         # AI agent guidelines
└── CLAUDE.md         # Symlink to AGENTS.md
```

See [docs/README.md](docs/README.md) for complete documentation index.

## Data Model

### Packages Table
```sql
CREATE TABLE packages (
    pkg_id      INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    epoch       INTEGER,
    version     TEXT NOT NULL,
    release     TEXT NOT NULL,
    arch        TEXT NOT NULL,
    summary     TEXT NOT NULL,
    description TEXT NOT NULL,
    repo        TEXT NOT NULL
);
```

### Dependencies
```sql
CREATE TABLE requires (...);
CREATE TABLE provides (...);
```

### Vector Embeddings
```sql
CREATE TABLE pkg_embedding (
    pkg_id    INTEGER PRIMARY KEY,
    embedding BLOB NOT NULL
);
```

## Design Philosophy

- **Accuracy First**: Structured metadata takes precedence
- **Semantic Guide**: Vector search provides discovery hints
- **Local Operation**: No external dependencies at runtime
- **Minimal Complexity**: Single binary deployment

## Performance

- Suitable for 10k-100k packages
- Search latency: milliseconds to tens of milliseconds
- Low memory footprint (disk-based storage)

## Extending

Future enhancements could include:
- Multiple repository namespaces
- chroot/sysroot integration
- Source code indexing (ctags/tree-sitter)
- Migration to dedicated vector DB (Qdrant)

## Development

### Run Tests
```bash
cargo test
```

## Debugging and Logging

This project uses structured logging with the `tracing` framework. Control log verbosity with the `RUST_LOG` environment variable:

```bash
# Default (info level) - minimal output
./rpm_repo_search stats

# Debug level - detailed internal operations
RUST_LOG=debug ./rpm_repo_search search "kernel"

# Trace level - all logs including dependencies
RUST_LOG=trace ./rpm_repo_search build-embeddings

# Module-specific logging
RUST_LOG=rpm_repo_search::api=debug ./rpm_repo_search index -f primary.xml.gz -r rocky9
```

**Available log levels**: `error`, `warn`, `info` (default), `debug`, `trace`

**Log features**:
- Structured fields: `count=123`, `repo=rocky9`
- Spans: Command-level context tracking
- Timestamps: ISO 8601 format
- Function-level instrumentation

## License

[Add your license here]

## Additional Documentation

For more detailed information:
- **Usage Guide**: [USAGE.md](docs/USAGE.md) - Complete usage instructions
- **MCP Server Guide**: [MCP_GUIDE.md](docs/MCP_GUIDE.md) - Model Context Protocol integration
- **Development**: [DEVELOPMENT.md](docs/DEVELOPMENT.md) - Development notes and architecture
- **SQLite-vec Bundling**: [SQLITE_VEC_BUNDLING.md](docs/SQLITE_VEC_BUNDLING.md) - Extension bundling strategies
- **Compression**: [COMPRESSION.md](docs/COMPRESSION.md) - Supported compression formats
- **Changelog**: [CHANGELOG.md](docs/CHANGELOG.md) - Version history and updates
- **Design Documents**: 
  - [High-level Design](docs/design/rpm_repo_vector_search_design.md) - Architecture overview
  - [Detailed Design](docs/design/rpm_repo_vector_search_detailed_design.md) - Implementation details
- **Agent Guide**: [AGENTS.md](AGENTS.md) - Guidelines for AI agents working on this project

## References

- [RPM Package Manager](https://rpm.org/)
- [rpm-md format](https://github.com/rpm-software-management/createrepo_c)
- [Candle Framework](https://github.com/huggingface/candle)
- [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2)
- [sqlite-vec](https://github.com/asg017/sqlite-vec) - SQLite vector search extension
