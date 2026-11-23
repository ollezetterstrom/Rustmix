# AGENT.md - Rustmix Developer & AI Agent Guide

> **Note for Future AI Agents:** This document explains both the internal code architecture and the external ecosystem/release mechanics for **Rustmix**. Read this before touching releases, packaging, or distribution.

---

## 1. Project Overview & Pivot History
- **Project Name:** Rustmix (formerly repomix-rs)
- **Repository:** [https://github.com/ollezetterstrom/Rustmix](https://github.com/ollezetterstrom/Rustmix)
- **Crate Directory:** `repomix-rs/`
- **Core Mission:** Ultra-fast, high-density codebase packer for frontier reasoning models (Gemini 2.5 Pro, Claude 3.7 Sonnet, OpenAI o3).
- **The Pivot (v2.0.0):** The project previously maintained 1:1 bug-for-bug compatibility with JavaScript Repomix. In v2.0.0, we pivoted away from JS parity to achieve:
  1. **Maximum Token Density:** Terse delimiters, BPE whitespace optimization, automatic content deduplication, and minified blob truncation.
  2. **Maximum Raw Speed:** Dropped JS UTF-16 collation, fused pipeline passes into single parallel Rayon scans, and stream buffered output directly to disk.
  3. **Zero-Dependency Native Distribution:** Dropped heavy Node.js runtime wrappers in favor of standalone binary distribution for Windows, Linux, and macOS.

---

## 2. Code Architecture & Core Innovations

### LLM-First Compact Output (`--style compact`, Default)
- **File Delimiters:** Terse `=== path/to/file.rs` header instead of verbose XML blocks.
- **O(1) Content Deduplication:** (`src/core/metrics.rs` & `src/output/compact.rs`): Files with identical content hashes emit `=== path/b (duplicate of path/a)` instead of duplicating tokens.
- **Whitespace Token Optimization:** (`src/core/indent.rs`): Replaces leading 4 spaces with tabs (`\t`), reducing BPE indentation tokens by up to 75% while keeping visual structure intact.
- **Blob Truncation:** (`src/core/base64.rs`): Truncates long inline SVG paths (`<path d="...">` > 60 chars) and minified single lines (> 500 chars).
- **Smart Compression:** (`--smart-compress [N]`): Uses Git commit counts (`src/core/git.rs`) to keep the top N most-edited files in full context while Tree-sitter compressing the rest (`src/core/compress.rs`).

### High-Speed Fused Pipeline
- **Fused Rayon Pass:** File reading, secret detection (Aho-Corasick + regex in `src/core/security.rs`), and content processing occur in a single parallel Rayon pass in `src/core/packager.rs`.
- **Buffered Output Streaming:** Writes directly to disk/stdout via `BufWriter` (`src/output/writer.rs`) with `CountingWriter`, avoiding multi-megabyte allocations.
- **Fast Byte Token Estimation:** Defaults to `bytes / 3.8` for console reports. Exact tiktoken counting is auto-enabled when `--exact-tokens`, `--token-budget`, or `--token-count-tree` is passed.
- **Native Lexicographical Collation:** Standard `a.cmp(b)` in `src/util/path.rs` (replaced slow UTF-16 JS `localeCompare`).

---

## 3. Distribution Ecosystem (Outside the Code)

### Why Not npm? (Context for Future AIs)
npm enforces mandatory Two-Factor Authentication (2FA) and strict organization scoping for package publishing. To keep Rustmix completely zero-dependency and eliminate JavaScript runtime requirements, Rustmix is distributed natively as compiled binaries and via Cargo.

### Channel 1: GitHub Releases (Standalone Binaries)
- **Workflow:** `.github/workflows/release.yml`
- **Trigger:** Pushing any Git tag matching `v*` (e.g., `v2.0.0`).
- **Target Matrix:**
  1. `x86_64-pc-windows-msvc` (built with static CRT `/MT` + `+crt-static` in `repomix-rs/.cargo/config.toml` so it requires zero Visual C++ DLLs).
  2. `x86_64-unknown-linux-gnu`
  3. `aarch64-unknown-linux-gnu` (runs native on `ubuntu-24.04-arm`)
  4. `aarch64-apple-darwin` (Apple Silicon M-series)
  5. `x86_64-apple-darwin` (Intel Mac)
- **Artifacts:** Standalone `.zip` for Windows and `.tar.gz` for Linux/macOS uploaded to GitHub Releases.

### Channel 2: Homebrew (macOS & Linux)
- **Tap Repository:** `ollezetterstrom/homebrew-rustmix`
- **Install Command:**
  ```bash
  brew install ollezetterstrom/rustmix/rustmix
  # or:
  brew tap ollezetterstrom/rustmix && brew install rustmix
  ```
- **Update Command:**
  ```bash
  brew upgrade rustmix
  ```

### Channel 3: One-Line Installers (Zero Dependencies)
- **Linux & macOS:**
  ```bash
  curl -fsSL https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.sh | sh
  ```
- **Windows (PowerShell):**
  ```powershell
  irm https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.ps1 | iex
  ```

### Channel 4: Built-in Self-Updater
- **Command:** `rustmix update`
- **Mechanism:** Built into `src/main.rs`. Invokes the platform's installer script to download the latest binary from GitHub Releases and replace the running binary in-place.

### Channel 5: Cargo / crates.io
- **Install Command:** `cargo install rustmix`
- **Publish Command:** `cargo publish --manifest-path repomix-rs/Cargo.toml`
- **Credentials:** Token stored in `~/.cargo/credentials.toml` (created Nov 23, 2026, expires Nov 23, 2027; metadata recorded in local `token-info.txt`).
- **Prerequisite:** Email verification on [crates.io/settings/profile](https://crates.io/settings/profile).

---

## 4. Release Checklist for Future AI Agents

When the user requests a new release (e.g. `v2.0.1` or `v2.1.0`):

1. **Bump Version:**
   - Update `version = "X.Y.Z"` in `repomix-rs/Cargo.toml`.
   - Update `Cargo.lock` by running `cargo check --manifest-path repomix-rs/Cargo.toml`.
2. **Run Tests:**
   - Run `cargo test --release --manifest-path repomix-rs/Cargo.toml` (must pass 100%).
   - Run `cargo publish --dry-run --manifest-path repomix-rs/Cargo.toml`.
3. **Commit & Push to Main:**
   - `git commit -am "chore: release vX.Y.Z"`
   - `git push origin main`
4. **Trigger GitHub Cloud Builds:**
   - `git tag vX.Y.Z`
   - `git push origin vX.Y.Z`
5. **Publish to Crates.io:**
   - `cargo publish --manifest-path repomix-rs/Cargo.toml`
6. **Update Homebrew Formula (if needed):**
   - Calculate sha256 checksums of the release archives and update `Formula/rustmix.rb` in `ollezetterstrom/homebrew-rustmix`.
