# AGENT.md - Rustmix Developer & AI Agent Guide

## 1. Project Overview
- **Project Name:** Rustmix (formerly repomix-rs)
- **Repository:** [https://github.com/ollezetterstrom/Rustmix](https://github.com/ollezetterstrom/Rustmix)
- **Core Purpose:** Ultra-fast, high-density codebase packer designed for frontier reasoning LLMs (Gemini 2.5 Pro, Claude 3.7 Sonnet, OpenAI o3).
- **Core Principles:** Maximum token density, native Rust execution speed (zero JS/Node runtime overhead), zero-friction cross-platform installation.

---

## 2. Architecture & Key Features

### Compact LLM-First Output (`--style compact`, Default)
- **Terse Delimiters:** Uses `=== path/to/file.rs` delimiters instead of verbose XML/Handlebars tags.
- **O(1) Content Fingerprint Deduplication:** Identical files (e.g. repeated vendor files, licenses) are fingerprinted via fast hashes and emitted as `=== path/b (duplicate of path/a)` rather than repeating contents.
- **Indentation Compression:** Leading 4-space indentation is automatically converted to tabs (`\t`) in `compact` mode (1 token per indent level for BPE tokenizers).
- **Blob Truncation:** Long inline SVG path data (`<path d="...">`) and minified single lines (> 500 characters) are truncated to prevent token window pollution.
- **Smart Compression:** `--smart-compress [N]` uses Git change frequency (`src/core/git.rs`) to retain the top N (default 10) most-edited files in full while AST compressing the rest of the codebase with Tree-sitter.

### Speed & Performance Optimizations
- **Fused Rayon Pipeline:** File reading, security scanning (regex/Aho-Corasick), and content processing are merged into a single parallel pass in `src/core/packager.rs` so memory stays CPU cache-hot.
- **Streaming Output:** Writes directly to disk or stdout via `BufWriter` with `CountingWriter` instead of building multi-megabyte strings in RAM.
- **Fast Token Estimation:** Defaults to fast byte estimate (`bytes / 3.8`) for console reports to avoid ~870ms of BPE overhead. Exact tiktoken counting is auto-enabled when `--exact-tokens`, `--token-budget`, or `--token-count-tree` is used.
- **Native Lexicographical Collation:** Replaced JavaScript UTF-16 localeCompare collation with native Rust path ordering.

---

## 3. Distribution & Release Guide

Rustmix is distributed through two channels:
1. **Cargo / crates.io:** For any developer with Rust installed (`cargo install rustmix`).
2. **GitHub Releases:** Standalone pre-compiled zero-dependency binaries for Windows, Linux, and macOS (installed via one-line shell / PowerShell scripts).

### Step A: Publishing to Crates.io (`cargo publish`)
- **What it does:** Uploads the crate to [crates.io/crates/rustmix](https://crates.io/crates/rustmix). Anyone can immediately run `cargo install rustmix`.
- **Command:**
  ```bash
  cargo publish --manifest-path repomix-rs/Cargo.toml
  ```
- **Requirements:**
  - Token in `~/.cargo/credentials.toml` (created Nov 23, 2026, expires Nov 23, 2027; details in local `token-info.txt`).
  - Verified email on crates.io: [https://crates.io/settings/profile](https://crates.io/settings/profile).

### Step B: Git Tagging & GitHub Actions Release
- **What it does:** Tags a release version in Git and pushes it to GitHub. This triggers the GitHub Actions workflow in `.github/workflows/release.yml`.
- **What GitHub Actions does automatically:**
  - Starts 5 cloud runners in parallel.
  - Builds release binaries for:
    - `x86_64-pc-windows-msvc` (with static CRT `/MT` + `+crt-static`, zero DLL dependencies)
    - `x86_64-unknown-linux-gnu`
    - `aarch64-unknown-linux-gnu` (native on `ubuntu-24.04-arm`)
    - `aarch64-apple-darwin` (Apple Silicon M-series)
    - `x86_64-apple-darwin` (Intel Mac)
  - Packages Windows `.zip` and Unix `.tar.gz` standalone archives.
  - Publishes them as a GitHub Release at `https://github.com/ollezetterstrom/Rustmix/releases`.
- **Command:**
  ```bash
  git tag v<version>
  git push origin v<version>
  # Example:
  git tag v2.0.0
  git push origin v2.0.0
  ```
- **When to do Step B:**
  - Tell Gemini CLI to run Step B whenever you are ready to ship a public release binary to end users.
  - Once Step B completes, the one-line installer scripts and `rustmix update` automatically fetch and install the new version.

---

## 4. Install & Update Commands

### End-User Installation:
- **Cargo (Fastest & Native):**
  ```bash
  cargo install rustmix
  ```
- **Linux & macOS (Zero dependencies):**
  ```bash
  curl -fsSL https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.sh | sh
  ```
- **Windows (PowerShell, zero dependencies):**
  ```powershell
  irm https://raw.githubusercontent.com/ollezetterstrom/Rustmix/main/install.ps1 | iex
  ```

### Updating:
```bash
rustmix update
```
*(Built directly into the binary — invokes the platform installer to replace the current binary with the latest GitHub release).*

---

## 5. Development & Testing Commands
- **Run all unit/integration tests:**
  ```bash
  cargo test --release --manifest-path repomix-rs/Cargo.toml
  ```
- **Dry-run publish check:**
  ```bash
  cargo publish --dry-run --manifest-path repomix-rs/Cargo.toml
  ```
- **Verify binary locally:**
  ```bash
  ./repomix-rs/target/release/rustmix --version
  ./repomix-rs/target/release/rustmix . -o rustmix-output.txt
  ```
