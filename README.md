# Rustmix

**Ultra-fast, high-density codebase packer for frontier reasoning models.**

Rustmix packs your repository into a token-optimized single file, built for frontier AI models like Gemini 2.5 Pro, Claude 3.7 Sonnet, and OpenAI o3. It delivers **maximum token density** and **blazing speed** by ditching legacy JS overhead, streaming directly to disk, and deduplicating redundant contents.

```bash
repomix .
```

```
src/
  main.rs
  core/
    packager.rs

=== src/main.rs
fn main() { ... }

=== src/core/packager.rs
pub fn pack() { ... }

=== vendor/dup.rs (duplicate of src/main.rs)
```

- **Maximum Token Density:** Terse `=== file` delimiters, leading 4-space to tab conversion (1 token per level), O(1) duplicate content deduplication, and minified blob truncation.
- **Instant Console Reporting:** Defaults to fast byte-based token estimation (`bytes / 3.8`), dropping ~870ms of BPE overhead.
- **Fused Pipeline & Streaming:** Merges file reading, security checks, and AST processing into a single parallel pass and streams directly to disk via `BufWriter` with minimal memory footprint.
- **Smart Compression:** Keep your most frequently changed files in full context while Tree-sitter compressing the rest of the repo.
- **Zero-Dependency One-Command Install:** Cross-platform single binaries for Windows, Linux, and macOS.

---

## ⚡ Installation

### Linux & macOS
```bash
curl -fsSL https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.sh | sh
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.ps1 | iex
```

### GitHub Releases
Grab the pre-compiled binary for your architecture from [GitHub Releases](https://github.com/ollezetterstrom/Rustmix/releases/latest):
- `x86_64-pc-windows-msvc` (Statically linked CRT, zero dependencies)
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin` (Apple Silicon M-series)
- `x86_64-apple-darwin` (Intel Mac)

### Cargo
```bash
cargo install --git https://github.com/ollezetterstrom/Rustmix.git repomix-rs
```

---

## 🚀 Usage

```bash
repomix                                # Pack repo using default compact style -> repomix-output.txt
repomix . --stdout                     # Stream output directly to stdout
repomix . --smart-compress 10          # Keep top 10 most changed files in full, compress the rest
repomix . --compress                   # Tree-sitter compress all compressible files
repomix . --exact-tokens               # Count exact BPE tokens via tiktoken (instead of fast estimate)
repomix . --style xml                  # Legacy Repomix XML output
repomix . --style markdown             # Markdown output
repomix . --token-budget 100000        # Error if output exceeds token budget (auto-enables exact tokens)
repomix . --include "src/**"           # Include specific globs
repomix . --ignore "**/*.test.*"       # Ignore patterns
```

---

## 💡 LLM-First Features

### 1. Compact Style (`--style compact`, Default)
The compact style removes all verbose XML tags, metadata boilerplate, and redundant guidelines. Instead, it emits:
- A clean, minimal directory tree.
- Terse file delimiters: `=== path/to/file.rs`.
- **O(1) Content Deduplication:** Identical files (e.g. repeated licenses or vendor packages) are fingerprinted in O(1) time and rendered as `=== path/b (duplicate of path/a)`.
- **Whitespace Token Optimization:** Automatically converts leading 4 spaces into tabs, reducing indentation tokens by up to 75% for modern BPE tokenizers.
- **Blob Truncation:** Truncates long inline SVG `<path d="...">` data and minified single lines (> 500 characters) to avoid polluting model context windows.

### 2. Smart Compression (`--smart-compress [N]`)
Uses Git commit frequency to identify the top `N` (default: 10) most actively developed files in your repository, keeping them completely intact for full semantic context while Tree-sitter compresses definitions, structs, and signatures across the remainder of the codebase.

### 3. High Speed Streaming Architecture
- **Fused Rayon Pass:** File reading, regex credential scanning, and content transformation occur in a single parallel pass while memory is hot in CPU cache.
- **Buffered Output Streaming:** Writes directly to disk or stdout via `BufWriter`, avoiding multi-megabyte string allocations.
- **Fast Byte Token Estimation:** Estimates token counts using empirical byte heuristics (`bytes / 3.8`), avoiding expensive tokenizer passes during standard builds. Exact counting is auto-enabled when `--exact-tokens`, `--token-budget`, or `--token-count-tree` is provided.

---

## 🛠️ Development

```bash
cd repomix-rs
cargo test --release
```

---

## License

MIT
