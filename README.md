# Rustmix

**Ultra-fast, high-density codebase packer for frontier reasoning models.**

Rustmix packs your repository into a token-optimized single file, built for frontier AI models like Gemini 2.5 Pro, Claude 3.7 Sonnet, and OpenAI o3. It delivers **maximum token density** and **blazing speed** by ditching legacy JS overhead, streaming directly to disk, and deduplicating redundant contents.

```bash
rustmix .
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
- **Zero-Dependency One-Command Install:** Cross-platform standalone binaries for Windows, Linux, and macOS.
- **Self-Updating:** Keep Rustmix up-to-date anytime by running `rustmix update`.

---

## ⚡ Installation

### Cargo (Cross-Platform)
```bash
cargo install rustmix
```

### Windows (PowerShell — Fresh PC, zero dependencies)
```powershell
irm olle.gay/Rustmix/install.ps1 | iex
```

### Windows Package Manager (winget)
> *PR pending in [microsoft/winget-pkgs#487407](https://github.com/microsoft/winget-pkgs/pull/487407)*
```powershell
winget install rustmix
```

### Linux & macOS (Shell — Fresh PC, zero dependencies)
```bash
curl -sSfL olle.gay/Rustmix/install.sh | sh
```

### Homebrew (macOS & Linux)
```bash
brew install ollezetterstrom/rustmix/rustmix
```

### Standalone GitHub Releases
Grab the pre-compiled binary for your architecture from [GitHub Releases](https://github.com/ollezetterstrom/Rustmix/releases/latest):
- `x86_64-pc-windows-msvc` (Statically linked CRT, zero dependencies)
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin` (Apple Silicon M-series)
- `x86_64-apple-darwin` (Intel Mac)

---

## 🔄 Updating

Once installed, updating Rustmix to the latest release is as simple as:

```bash
rustmix update
```

*(Works on Windows, Linux, and macOS — automatically fetches and installs the latest binary release).*

---

## 🚀 Usage

```bash
rustmix                                # Pack repo using default compact style -> rustmix-output.txt
rustmix . --stdout                     # Stream output directly to stdout
rustmix . --smart-compress 10          # Keep top 10 most changed files in full, compress the rest
rustmix . --compress                   # Tree-sitter compress all compressible files
rustmix . --exact-tokens               # Count exact BPE tokens via tiktoken (instead of fast estimate)
rustmix . --style xml                  # XML output (<rustmix>)
rustmix . --style markdown             # Markdown output
rustmix . --token-budget 100000        # Error if output exceeds token budget (auto-enables exact tokens)
rustmix . --include "src/**"           # Include specific globs
rustmix . --ignore "**/*.test.*"       # Ignore patterns
rustmix update                         # Update rustmix to latest release
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
