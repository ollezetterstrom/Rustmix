# repomix-rs — Engineering Context

> Purpose of this document: hand it to a strong reasoning model together with
> the prompt at the bottom, and get back a concrete, correct **performance
> optimization plan** for `repomix-rs`.
>
> Everything below is measured or verified on this machine. Do not assume
> numbers you cannot find here.
>
> **Revision 2 (2026-10-21):** the previous revision contained a *wrong
> baseline* — the benchmark harness was measuring the wrong directory and
> never actually passing `--stdout`. See §4.1. All numbers in this revision
> are from the fixed harness, which now verifies that both implementations
> packed the same tree.

---

## 1. What the project is

`repomix-rs` is a **from-scratch Rust re-implementation of
[Repomix](https://github.com/yamadashy/repomix) v1.18.1** — a tool that packs a
codebase into a single AI-friendly file (XML / Markdown / JSON / Plain).

Hard requirements, in priority order:

1. **Drop-in CLI compatibility.** Same flags, same output formats, and
   *byte-identical output files* to the official TypeScript implementation.
2. **Speed.** Target: <15 ms cold start, 10–30× faster throughput than the TS
   original.
3. **Single statically-linked binary**, minimal dependency surface.
4. **No behaviour regressions.** Any change must keep the differential test
   suites green (see §6).

Reference implementation used for differential testing: the published npm
package `repomix@1.18.1` (installed via `npx repomix`). Its compiled JS lives
at `C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules\repomix\lib\`
— **read it freely, it is the ground truth for every behavioural quirk.**

---

## 2. Repository layout

```
C:\Users\ozett\Documents\Projects\Rustmix\
├─ repomix-rs\                  # the Rust crate
│  ├─ Cargo.toml
│  ├─ src\
│  │  ├─ main.rs                # CLI entry, orchestration, reporting
│  │  ├─ lib.rs                 # library facade (modules are `pub`)
│  │  ├─ cli.rs                 # clap derive definitions + arg normalisation
│  │  ├─ config.rs              # config file schema, defaults, merge rules
│  │  ├─ report.rs              # terminal report blocks
│  │  ├─ core\
│  │  │  ├─ mod.rs
│  │  │  ├─ packager.rs          # THE PIPELINE: search→read→check→process→sort→render→metrics
│  │  │  ├─ search.rs            # directory walk + glob include/ignore filtering
│  │  │  ├─ read.rs              # file read, binary detection, BOM handling
│  │  │  ├─ binary.rs (util)    # binary-extension table, NUL probe, BOM table
│  │  │  ├─ security.rs          # 36 secret-detection rules (secretlint port)
│  │  │  ├─ comments.rs          # comment stripping per language
│  │  │  ├─ compress.rs          # tree-sitter compression, 17 languages, 5 strategies
│  │  │  ├─ base64.rs            # --truncate-base64
│  │  │  ├─ decorations.rs       # file_summary / notes / header text generation
│  │  │  ├─ tree.rs              # directory-structure tree rendering
│  │  │  ├─ git.rs               # git diffs / logs / change-frequency
│  │  │  └─ metrics.rs           # token counting
│  │  ├─ output\
│  │  │  ├─ mod.rs               # RenderContext, delimiter calc, style dispatch
│  │  │  ├─ xml.rs               # XML + parsable-XML (fast-xml-builder clone)
│  │  │  ├─ markdown.rs
│  │  │  ├─ json.rs
│  │  │  ├─ plain.rs
│  │  │  ├─ style_utils.rs       # extension→language map, XML escaping, js_trim
│  │  │  └─ writer.rs            # file/stdout/clipboard write, --split-output
│  │  └─ util\
│  │     ├─ path.rs              # POSIX normalisation, ICU-like collation, sortPaths
│  │     ├─ glob.rs              # minimatch/picomatch-compatible glob engine
│  │     └─ size.rs              # "500kb"/"2.5mb" parsing
│  ├─ src\bin\                   # bench_prims.rs, calib.rs, regexbench.rs (dev tools)
│  ├─ tests\                     # integration tests
│  └─ target\release\repomix.exe # the binary
├─ bench.ps1                    # benchmark harness (Rust vs npx repomix)
├─ compare-all.ps1              # 26-case OUTPUT differential test vs npx repomix
├─ compare-metrics.ps1          # 8-case REPORT differential test (token counts!)
├─ CONTEXT.md                   # this file
├─ PROMPT.md                    # the prompt for the analysis model
└─ testdata\                    # small fixture repo (incl. a unicode fixture)
```

---

## 3. The pipeline (core/packager.rs)

`pack(root_dirs, cfg, progress) -> PackResult` runs these phases **in order**:

| # | Phase | Parallelism | Notes |
|---|-------|-------------|-------|
| 1 | search | `rayon` over roots | `ignore::WalkBuilder` + custom glob matcher |
| 2 | read | `rayon` over files | `std::fs::read`, binary/BOM detection |
| 3 | git diffs/logs | serial | one `git` subprocess each (only if enabled) |
| 4 | security scan | `rayon` over files | 36 rules, gated by an aho-corasick literal prefilter |
| 5 | process content | `rayon` over files | remove-comments → compress → truncate-base64 → remove-empty-lines → trim → line-numbers |
| 6 | git sort | serial | stable sort by change count (the `git log` runs in a background thread) |
| 7 | tree build | serial | directory-structure string |
| 8 | render | serial | XML/Markdown/JSON/Plain string building |
| 9 | metrics | `rayon` | per-file token counts **and** the output token count — **skipped entirely when the result is not consumed** |

Phase timings are instrumented behind `REPOMIX_TIMING=1` (prints `[timing]`
lines to stderr). `phase!()` macro is in `packager.rs`. The search phase has
finer instrumentation (`walk` / `match` / `sort`).

---

## 4. Measured performance (this machine: Windows 11, x86-64, release build)

Corpus A = `C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules`
— 6 907 files, 90 MB, JS/TS-heavy. Packed output: **65 464 013 bytes**.

### End-to-end (Corpus A, `--stdout`, median of 3, `bench.ps1`)

| Implementation | Wall time | Notes |
|---|---|---|
| **repomix-rs** | **426 ms** | no cache, every run identical |
| repomix (TS) | 2 453 ms | **with warm token-count disk cache** |
| repomix (TS) | 5 182 ms | **cold token-count cache** (the honest "first run" number) |

> **Speedup: 5.8× (warm) / 12.2× (cold).** The project goal (10–30×) is met
> against the cold-cache comparison.

### Cold start

`repomix.exe --version` → **7.0 – 7.7 ms** (best of 5). Target <15 ms: **met**.

### Phase breakdown (Corpus A, single run, `REPOMIX_TIMING=1`)

```
[timing]   walk=88ms match=19ms sort=4ms files=6907
[timing] search                    93.4ms
[timing] setup                     95.1ms
[timing] read                     139.8ms
[timing] security                 193.9ms
[timing] process                  201.8ms
[timing] sort                     205.6ms
[timing] render                   217.3ms
[timing] metrics                  217.4ms     <- skipped (--stdout)
```

With the report enabled (metrics consumed) the metrics phase is **~1 050 ms**
(94 ms of that is the BPE table build, now overlapped with the file I/O).

### Where the time goes now

1. **Directory walk — 88 ms.** Pure syscall bound (`readdir` + `stat` for
   6 907 files and ~2 500 directories). Parallelising the `ignore` crate's
   walker gave **no** improvement; this is the floor for a stat-based walk.
2. **Reading — 45 ms.** `std::fs::read` on 6 907 files.
3. **Secret scan — 54 ms** (parallel). Was 144 ms.
4. **Render — 12 ms.** Was 93 ms.
5. **Writing the 65 MB to stdout — ~175 ms.** A single `write_all`; the cost
   is the pipe transfer, and the TS pays it too.
6. **Fixed costs (paid by every run, independent of corpus size):**
   - regex compilation of the security rules: **122 ms → 1.4 ms** (lazy
     compilation, §5).
   - BPE table build: 94 ms → hidden behind the file I/O.

### 4.1 Benchmark methodology (READ THIS — the harness was broken before)

The previous revision of this document reported "319 ms" for the Rust
implementation and "590 ms" for the TS one. **Both numbers were wrong**, for
two independent reasons, both of which were bugs in the PowerShell harnesses:

1. **`Push-Location` does not change the native process working directory.**
   PowerShell keeps its own location stack; `[Environment]::CurrentDirectory`
   stays put. Every child process started via `System.Diagnostics.Process`
   without an explicit `WorkingDirectory` therefore inherited the *shell's*
   original directory — not the directory that was pushed to. Both `bench.ps1`
   and `compare-all.ps1` were packing the Rustmix project, not the corpus.
2. **`Quote-Args` had a parameter called `$Args`**, which collides with
   PowerShell's automatic `$args` variable. The function silently returned an
   **empty string**, so the tools were never actually given `--stdout`:
   the Rust binary packed to a file and printed a 2 954-char report, and the
   JS side ran bare `npx` with no `repomix` subcommand.
3. In `compare-all.ps1`, the local variable `$rs` was the **same variable**
   as the parameter `$Rs` (PowerShell variables are case-insensitive), so the
   exe path was overwritten by the first result and every later case failed
   to launch.

Both harnesses are fixed and now **verify their own workload**: `bench.ps1`
compares the output size of the Rust and the JS run and fails loudly if they
differ, and `compare-all.ps1` starts every child with an explicit
`WorkingDirectory`.

> The TS implementation caches per-file token counts in
> `%TEMP%\repomix\cache\token-counts.json` (md5-keyed). That is why its warm
> number is 2× better than its cold number. `bench.ps1 -ClearTokenCache`
> clears it for a cold measurement; the reported "cold" row does this.
> `repomix-rs` deliberately has **no** disk cache — every run pays the full
> BPE price.

---

## 5. The output-compatibility contract (DO NOT BREAK THESE)

### 5.1 The differential suites

| Suite | What it compares | Coverage |
|---|---|---|
| `compare-all.ps1` | the **packed output** (stdout, `--stdout` mode), 26 flag combinations | the output files |
| `compare-metrics.ps1` | the **numbers in the console report**, 8 flag combinations | total/per-file token counts, char counts, file counts, security findings |

**Both currently pass 26/26 and 8/8.** `compare-metrics.ps1` did not exist
before: the old suite only compared stdout in `--stdout` mode, so the report
— and with it the *entire* metrics phase — was never verified. It caught two
real bugs (see §5.3).

### 5.2 Load-bearing quirks

* **Handlebars standalone-block whitespace.** The TS output is produced by
  Handlebars templates. A block helper alone on its line (`{{#if x}}`,
  `{{/if}}`, `{{#each}}`, `{{/each}}`) causes that whole line — including its
  newline — to be removed. This is why e.g. `</file_summary>` is followed by
  exactly one blank line. Reproduced by hand in `output/xml.rs`,
  `output/markdown.rs`, `output/plain.rs`.
* **`fast-xml-builder` escaping** for `--parsable-style`: `&`→`&amp;`,
  `<`→`&lt;`, `>`→`&gt;`, `"`→`&quot;`, `'`→`&apos;`; no indentation, no XML
  declaration; `<files>` children are emitted as repeated `<file path="...">`
  elements; `<files>` inside a git-log commit is emitted as a *repeated*
  `<files>` element, not a wrapper.
* **`picomatch` trailing-`/**` rule.** A pattern ending in `/**` matches the
  path *itself* only when the preceding segment does not end in a wildcard.
  So `**/Cargo.lock/**` matches `Cargo.lock` (it is ignored) but
  `**/repomix-output.*/**` does **not** match `repomix-output.xml` (so the
  output file is *included* in the pack — a real repomix quirk). Implemented
  via `Pattern::trailing_needs_slash` in `util/glob.rs`.
* **`normalizeGlobPattern`** (Repomix-specific): `**/X` → `**/X/**`;
  `dir/` → `dir`. Applied to *ignore* patterns only, never to include
  patterns.
* **ICU root collation** for path sorting (`localeCompare`). Primary weights
  of the *whole* string are compared before case, so `COMMIT_EDITMSG` sorts
  before `config` while `a` sorts before `A`. Implemented in
  `util/path.rs::locale_cmp`. The optimised (allocation-free) version is
  proven equivalent to the original by a unit test that compares both
  implementations over 900 input pairs.
* **`sortPaths` segment comparison**: at the first differing segment, a path
  that *ends* there sorts *after* one that continues (files below their
  directory).
* **secretlint "recommend" preset** is much stricter than its raw regexes:
  the AWS *access-key-id* rule is **disabled by default**
  (`enableIDScanRule: false`); private keys need a full PEM block with
  ≥100 base64 chars starting with `MI` or `b3BlbnNzaC1rZXktdjE`; SendGrid keys
  must be exactly 69 chars; the npm OAuth rule only applies inside
  `package.json`/`package-lock.json`; the npm `_authToken` rule only inside
  `.npmrc`. A calibration harness (`src/bin/calib.rs` + `work/sl-scan.cjs`)
  compares the two implementations rule-by-rule.
* **Look-around emulation.** Rust's `regex` crate has no look-around, so
  `(?<!\p{L})` / `(?![A-Za-z0-9_])` are emulated by hand in
  `core/security.rs::find_guarded`.
* **Token counts in the summary** must match the TS numbers, which use
  gpt-tokenizer's `o200k_base` and a *fast path*
  (`sum(perFileTokens) + wrapperTokens`) rather than tokenizing the whole
  output. The fast path applies only when `splitOutput` is unset,
  `parsableStyle` is off, the style is xml/markdown/plain, and there is a
  single output part; the "wrapper" is recovered by locating each file's
  content inside the rendered output with `indexOf` and counting the tokens
  of the gaps. `tiktoken-rs` `encode_ordinary` is the Rust equivalent.
  **This is a correctness requirement, not just an optimisation:** counting
  the whole output merges tokens across the file boundaries and reported
  663 tokens where the reference reports 660.
* **Character counts are UTF-16 code units**, because the reference is
  JavaScript and `String.length` counts UTF-16 units. Counting Unicode
  scalar values reported 1 759 characters where the reference reports
  1 762 for a file containing three astral-plane characters.
* **Markdown code fence** = `max(3, longest_backtick_run + 1)` computed over
  *all* file contents, the tree string and the git diffs.
* **`--remove-comments` is a deliberate divergence.** The TS `strip-comments`
  package has a tokenizer bug (an unbalanced `"` makes it swallow the rest of
  the file, silently keeping every later comment) and the project has decided
  **not** to reproduce that bug. The Rust implementation strips comments
  *correctly*. **Do not propose "fixing" this to match the TS output.**
* **The console report's number formatting is locale-dependent.** The
  reference uses `Number.toLocaleString()`, so the group separator depends on
  the machine's locale (this machine is `en-SE` and uses U+252C; a normal
  en-US machine uses a comma, which is what the Rust implementation emits).
  `compare-metrics.ps1` compares the digits only.

### 5.3 Bugs found and fixed while optimising (2026-10-21)

* **Token total was wrong** (663 instead of 660) — the fast path was missing.
* **Character total was wrong** for astral-plane characters — UTF-16 vs
  scalar counting.
* **Non-ASCII text was corrupted by `--remove-comments`**: the stripper
  worked on bytes and pushed every byte as a Latin-1 character, so "åäö"
  became "Ã¥Ã¤Ã¶". Fixed by copying whole characters
  (`comments.rs::push_char_at`).
* **Panic in `compress.rs::ts_function_name`**: `line[8..]` panicked for
  every line shorter than eight bytes — i.e. almost every JS/TS file. With
  `panic = "abort"` the process died with no message. Fixed with
  `line.get(8..)`.
* **`js_trim` trimmed `U+0085`**, which JavaScript's `String.prototype.trim`
  does not. The exact ECMAScript whitespace set is now used
  (`style_utils::is_js_whitespace`).

---

## 6. Verification commands

```powershell
# unit + integration tests (53)
cd repomix-rs; cargo test --release

# output differential: 26 flag combinations vs npx repomix (byte comparison)
.\compare-all.ps1 -Dir C:\Users\ozett\Documents\Projects\Rustmix\testdata `
                  -RustExe C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe

# report differential: 8 flag combinations (token/char/file counts, security)
.\compare-metrics.ps1 -Dir C:\Users\ozett\Documents\Projects\Rustmix\testdata `
                      -RustExe C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe

# benchmark (Rust vs npx). NOTE: npx must be invoked through cmd.exe.
.\bench.ps1 -Target C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules -Runs 3

# phase timings
$env:REPOMIX_TIMING="1"; repomix.exe --stdout 2>timing.txt > out.xml

# micro-benchmarks of the hot primitives
cargo build --release --bin bench_prims
.\target\release\bench_prims.exe <some-file>

# per-rule regex compile cost
cargo build --release --bin regexbench
.\target\release\regexbench.exe
```

To clear the TS token cache (for cold-cache comparisons):
`Remove-Item "$env:TEMP\repomix\cache\token-counts.json" -Force`

---

## 7. Current dependencies

```
clap 4.6 (derive)      ignore 0.4      globset 0.4      rayon 1.12
tiktoken-rs 0.12       simdutf8 0.1    tree-sitter 0.27 + 17 grammars
serde/serde_json/json5 regex 1.13      aho-corasick 1 (new: security prefilter)
arboard 3.7            anyhow  thiserror  log 0.4  env_logger 0.11
unicode-width 0.2      windows-sys 0.61
```

Release profile: `opt-level=3, lto="fat", codegen-units=1, panic="abort", strip=true`.

**Dependency policy:** adding a small, well-maintained crate is acceptable
(`aho-corasick`, `memchr`, `rayon` are already in the tree transitively).
Avoid anything that would change observable behaviour.

---

## 8. Known open issues / risks

* **The directory walk (88 ms) is syscall bound.** The `ignore` crate's
  parallel walker made no difference. A hand-written walker that avoids the
  per-entry `stat` (by using `read_dir` + `file_type`) might, but that would
  mean reimplementing `.gitignore` handling.
* **`util/glob.rs` is a hand-written backtracking matcher.** It is correct for
  the cases that matter, but it is O(patterns × path segments) per file. It
  now only accounts for 19 ms of the 93 ms search phase, so it is *not* worth
  optimising further. `globset` cannot be swapped in blindly: its `**`
  semantics differ from picomatch's.
* **The `aho-corasick` prefilter is a correctness-critical optimisation.** It
  relies on every rule having a *necessary* literal. Two unit tests guard
  this: `prefilter_never_hides_a_matching_rule` (for every rule and every
  sample, a rule that matches must be in the prefilter's mask) and
  `every_match_contains_one_of_its_literals`. **Any new rule must come with
  a literal and a sample.**
* **The metrics phase is skipped when its result is not consumed**
  (`--stdout`, `--quiet`, no `--token-budget`). The library API keeps
  `Config::need_metrics = true` by default, so `pack()` still returns full
  metrics for library users.
* `panic = "abort"` means any panic kills the process — there is no error
  recovery. Two panics were found and fixed in 2026-10-21; more may lurk.
* `core/comments.rs` diverges from `strip-comments` in edge cases (accepted).
* Windows-only so far; path handling is POSIX-normalised everywhere, but the
  code has not been exercised on Linux/macOS.
* The `ignore` crate's `is_git_repository` probe was removed from the git
  helpers (it doubled the number of process spawns and `git log`/`git diff`
  fail in exactly the same situations).
* **Not implemented: content de-duplication in the metrics phase.** A
  `node_modules` tree contains many identical files (`LICENSE`, repeated
  `.d.ts`), and each copy is tokenised separately. Hashing the content
  would cost roughly as much as it saves, so a *prefix* hash (length +
  first/last 64 bytes) with a full `memcmp` verification would be needed.
  It is only worth ~100–200 ms in the report mode, which is no longer the
  critical path, and a hash collision would silently produce wrong token
  counts. Deliberately left out.

---

## 9. What I want from you

Produce a **prioritised, concrete optimization plan** with:

1. **Expected speedup per change**, with the reasoning (what work is removed).
2. **Concrete implementation sketches** (function signatures, which file, which
   data structure) — not just "use a faster hashmap".
3. **Correctness risks** for each change, specifically with respect to the
   output-compatibility contract in §5 and the differential suites in §6.
4. **Anything you believe is currently wrong or fragile** in the hot paths
   (the code is in the tree — read it).
5. **A ranked "do this first" list** of at most 5 items.

Priorities: (a) token counting, (b) secret scan, (c) search/glob matching,
(d) render, (e) cold start. Do not propose behaviour changes; do not propose
caching that changes output. If you think a measurement is missing, say
exactly which one and how to produce it with the tools in §6.
