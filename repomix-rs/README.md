# repomix-rs

A high-performance, drop-in compatible rewrite of
[Repomix](https://github.com/yamadashy/repomix) v1.18.1 in Rust.

It packs a repository into a single AI-friendly file (XML, Markdown, JSON or
Plain) and produces **byte-identical output** to the original TypeScript
implementation.

```
repomix .
```

## Install

**Windows** — one file, no runtime needed, no redistributable:

```powershell
.\install.ps1
```

The script builds the binary, copies it to `%LOCALAPPDATA%\Programs\repomix`
and adds that directory to your user `PATH`. Open a new terminal afterwards
and `repomix` is available.

You can also just copy `repomix.exe` anywhere and put that directory on the
`PATH` yourself - the binary is statically linked and depends on nothing but
the Windows system libraries.

**With cargo** (builds from source, needs a Rust toolchain and a C compiler
for the tree-sitter grammars):

```powershell
cargo install --path .
```

## Use

```powershell
repomix                          # pack the current directory
repomix . --stdout               # write the packed output to stdout
repomix src --style markdown     # a different format
repomix . --compress             # keep only the structure of each file
repomix . --include "src/**"     # only these files
repomix . -o packed.xml          # write it to a file
repomix . --no-security-check    # skip the secret scan
```

`repomix --help` lists every flag; they are the same ones the original
implementation has.

## Why

Measured against `npx repomix` on a 6 907 file / 90 MB repository
(`node_modules`), packing it into a 62 MB document:

| | repomix-rs | repomix (TypeScript) | |
|---|---|---|---|
| Runtime | **407 ms** | 4 951 ms | **12.2x** |
| Runtime, reference with a warm token cache | **407 ms** | 2 425 ms | **6.0x** |
| Peak memory | **196 MB** | 722 MB | **3.7x less** |
| Cold start | **7 ms** | ~400 ms | |
| Install footprint | one 32.6 MB file | Node + 6 925 files / 87 MB | |
| Files left behind | none | a token cache in `%TEMP%` | |

The Rust implementation keeps **no cache on disk**: every run does the full
work, and it is still faster than the original with its cache warm.

## Compatibility

The output is verified against the original implementation by three
differential test suites - 49 flag combinations in total, comparing the
packed output byte for byte, the numbers of the console report, and the
behaviour of the remaining flags.

Three deliberate differences are documented in `CONTEXT.md`:

* comments are stripped *correctly* (the original has a tokenizer bug that
  silently keeps every comment after an unbalanced quote),
* `--stdin` works on Windows (the original packs an empty repository there),
* the console report uses a comma as the thousands separator.

## Development

```powershell
cd repomix-rs
cargo test --release                # 55 unit and integration tests

# the differential suites (need node and the reference implementation)
.\compare-all.ps1    -Dir .\testdata -RustExe .\repomix-rs\target\release\repomix.exe
.\compare-metrics.ps1 -Dir .\testdata -RustExe .\repomix-rs\target\release\repomix.exe
.\compare-flags.ps1  -RustExe .\repomix-rs\target\release\repomix.exe
.\make-fixtures.ps1                 # the fixtures the suites need

# the benchmark
.\bench.ps1 -Target <some-directory> -Runs 3

# the phase timings
$env:REPOMIX_TIMING = "1"; repomix . --stdout > out.xml
```

`CONTEXT.md` describes the architecture, the measured performance and the
output-compatibility contract in detail.

## License

MIT
