# Rustmix

**Repomix, rebuilt in Rust.** Packs a repository into a single AI-friendly
file - and does it **12x faster** than the original TypeScript implementation,
with byte-identical output.

```powershell
repomix .
```

```
This file is a merged representation of the entire codebase, combined into a
single document by Repomix.

<file_summary>
...
```

- **12.2x faster** than `npx repomix` (402 ms vs 4 916 ms on a 6 907 file
  repository)
- **3.7x less memory** (196 MB vs 722 MB peak)
- **One self-contained binary** - 32.7 MB, no Node runtime, no Visual C++
  redistributable, nothing left behind on disk
- **Byte-identical output**, verified against the original implementation by
  49 differential test cases

## Install

### Download the binary

Grab `repomix-<version>-windows-x64.zip` from the
[releases](https://github.com/ollezetterstrom/Rustmix/releases), unzip it
anywhere and put that directory on your `PATH`. The binary is statically
linked, so it runs on any Windows machine.

### Or build it yourself

```bash
git clone https://github.com/ollezetterstrom/Rustmix.git
cd Rustmix/repomix-rs
cargo build --release
```

You need a Rust toolchain and a C compiler (the [tree-sitter][tree-sitter]
grammars are compiled from C). The build works on Windows, Linux and macOS -
the CI matrix builds and tests all three.

The result is `repomix-rs/target/release/repomix` (`.exe` on Windows).

### Or install it on your `PATH`

**Windows:**

```powershell
.\install.ps1
```

The script builds the binary, copies it to `%LOCALAPPDATA%\Programs\repomix`
and adds that directory to your user `PATH`. Open a new terminal afterwards.

**Any platform with cargo:**

```bash
cargo install --path repomix-rs
```

That installs into `~/.cargo/bin`, which is usually already on the `PATH`.

## Use

```bash
repomix                          # pack the current directory
repomix . --stdout               # write the packed output to stdout
repomix src --style markdown     # a different format
repomix . --compress             # keep only the structure of each file
repomix . --include "src/**"     # only these files
repomix . --ignore "**/*.test.*" # but not these
repomix . -o packed.xml          # write it to a file
repomix . --no-security-check    # skip the secret scan
repomix . --token-budget 50000   # fail if the result is too large
```

`repomix --help` lists every flag - they are the same ones the original
implementation has.

## How is it different from Repomix?

Only in speed and in how it is distributed. The packed output is
**byte-identical** to `npx repomix`, which is verified automatically:

| Suite | What it compares | Cases |
|---|---|---|
| `compare-all.ps1` | the packed output, byte for byte | 26 flag combinations |
| `compare-metrics.ps1` | the numbers of the console report (token counts, character counts, file counts, security findings) | 8 flag combinations |
| `compare-flags.ps1` | the remaining flags (git flags, `--stdin`, `--split-output`, `--token-budget`, ...) | 15 cases |

There are three deliberate differences, all documented in
[`CONTEXT.md`](CONTEXT.md):

- comments are stripped **correctly** - the original has a tokenizer bug that
  silently keeps every comment after an unbalanced quote,
- `--stdin` works on Windows - the original packs an empty repository there,
  because it builds its glob pattern from a path with backslashes in it,
- the console report uses a comma as the thousands separator, instead of the
  locale dependent one the original uses.

## Performance

Measured against `npx repomix` on a 6 907 file / 90 MB repository, packed
into a 62 MB document. The full methodology is in
[`CONTEXT.md`](CONTEXT.md) §4.

| | Rustmix | Repomix (TypeScript) | |
|---|---|---|---|
| Runtime | **402 ms** | 4 916 ms | **12.2x** |
| Runtime, reference with a warm token cache | **402 ms** | 2 403 ms | **6.0x** |
| Peak memory | **196 MB** | 722 MB | **3.7x less** |
| Cold start | **7 ms** | ~400 ms | |
| Install footprint | one 32.7 MB file | Node + 6 925 files / 87 MB | |
| Files left on disk | none | a token cache in `%TEMP%` | |

Rustmix keeps **no cache on disk**: every run does the full work, and it is
still faster than the original with its cache warm.

## Development

```bash
cd repomix-rs
cargo test --release                # 55 unit and integration tests
```

The differential suites compare against the original implementation and need
Node and the reference package:

```powershell
# the fixtures the suites need (the git fixture is a real repository)
.\make-fixtures.ps1

# the packed output, byte for byte
.\compare-all.ps1    -Dir .\testdata -RustExe .\repomix-rs\target\release\repomix.exe
# the numbers of the console report
.\compare-metrics.ps1 -Dir .\testdata -RustExe .\repomix-rs\target\release\repomix.exe
# the remaining flags
.\compare-flags.ps1  -RustExe .\repomix-rs\target\release\repomix.exe

# the benchmark
.\bench.ps1 -Target <some-directory> -Runs 3
```

The phase timings of a single run:

```powershell
$env:REPOMIX_TIMING = "1"; repomix . --stdout > out.xml
```

[`CONTEXT.md`](CONTEXT.md) describes the architecture, the measured
performance, the output-compatibility contract and the open issues in detail.

## Credits

The original implementation is [Repomix](https://github.com/yamadashy/repomix)
by yamadashy - this is a from-scratch Rust rewrite of it, and the output
compatibility is the point.

## License

MIT

[tree-sitter]: https://tree-sitter.github.io/tree-sitter/
