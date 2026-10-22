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
├─ compare-flags.ps1            # 15-case differential test for the remaining flags
├─ make-fixtures.ps1            # creates the fixture repositories the suites use
├─ install.ps1                  # installs the binary and puts it on the PATH
├─ portable-check.ps1           # lists the DLLs a PE binary imports
├─ CONTEXT.md                   # this file
├─ PROMPT.md                    # the prompt for the analysis model
├─ testdata\                    # small fixture repo (incl. a unicode fixture)
├─ testdata-git\                # git fixture (generated, not committed)
├─ testdata-config\             # config file fixture
├─ testdata-multi\              # two directories, for the multi root test
└─ testdata-encoding\           # Latin-1, CRLF and BOM files
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
| **repomix-rs** | **364 ms** | no cache, every run identical |
| repomix (TS) | 2 425 ms | **with warm token-count disk cache** |
| repomix (TS) | 5 145 ms | **cold token-count cache** (the honest "first run" number) |

> **Speedup: 6.7× (warm) / 14.2× (cold).** The project goal (10–30×) is met
> against the cold-cache comparison.

The **default run** (the output goes to a file and the report is printed, so
the metrics phase runs) went from **1 568 ms to 930 ms** with the
optimisations of §4.2.

### Cold start

`repomix.exe --version` → **6.8 – 7.2 ms** (best of 5). Target <15 ms: **met**.

### Phase breakdown (Corpus A, single run, `REPOMIX_TIMING=1`)

```
[timing]   walk=78ms match=5.5ms sort=4ms files=6907
[timing] search                    83.0ms
[timing] setup                    85.0ms
[timing] read                     130.0ms
[timing] security                 183.0ms
[timing] process                  183.5ms
[timing] sort                     187.0ms
[timing] render                   199.0ms
[timing] metrics                  199.0ms     <- skipped (--stdout)
[timing] pack (in run)            205.0ms
[timing] write output             110.0ms
[timing] total (in main)          316.0ms
```

The `REPOMIX_TIMING` output also reports the time outside of `main()` (the
process start and the teardown of the address space); `measure-teardown.ps1`
compares it with the wall clock.

### Where the time goes now

1. **Directory walk — 78 ms.** Pure syscall bound (`readdir` + `stat` for
   6 907 files and ~2 500 directories). Parallelising the `ignore` crate's
   walker gave **no** improvement; this is the floor for a stat-based walk.
2. **Reading — 45 ms.** `std::fs::read` on 6 907 files.
3. **Secret scan — 53 ms** (parallel). Was 146 ms without a prefilter.
4. **Render — 12 ms.** Was 93 ms.
5. **Writing the 65 MB to stdout — ~105 ms.** A single `write_all`; the cost
   is the pipe transfer, and the TS pays it too.
6. **Process teardown — 49 ms** outside of `main()`. The operating system
   unmaps the 172 MB of address space when the process exits; skipping the
   `free()` of the result (`mem::forget`) does not change it.
7. **Fixed costs (paid by every run, independent of corpus size):**
   - regex compilation of the security rules: **122 ms → 1.4 ms** (lazy
     compilation, §5).
   - BPE table build: 94 ms → hidden behind the file I/O.

### 4.2 The metrics phase (the report path)

The default run (`repomix .`, which writes the file **and** prints the
report) is the one a user actually sees, and it is dominated by the token
counting. The phase went from **1 433 ms to 872 ms**:

| Part | Before | After | What was done |
|---|---|---|---|
| Per-file token counts | 760 ms | 700 ms | identical contents are only counted once (1 519 of the 6 907 files are duplicates) and the largest file is handed to the thread pool first |
| Per-file UTF-16 length | 232 ms | ~10 ms | it was counted **sequentially** over the whole corpus; it is now counted in the same parallel pass and derived from the bytes instead of decoding the text |
| The wrapper | 138 ms | ~15 ms | `str::find` builds a two-way searcher for every needle (as much work as the whole corpus); it is now searched with `memchr` and compared directly |
| The output | 29 ms | 29 ms | the UTF-16 length of the rendered output |

**The token counting itself is the floor.** `tiktoken-rs` spends ~70 % of its
time in the `fancy_regex` pre-tokenisation of the `o200k_base` pattern, and
that part cannot be replaced without reimplementing the BPE as well (the
merger and the rank table are private in `tiktoken-rs`). See §8.

### 4.3 What was measured and rejected

Further optimisations were implemented and measured on Corpus A. Only the
ones that paid off were kept:

| Idea | Result |
|---|---|
| **Content de-duplication** in the token counting (group identical files, tokenise each body once) | **Kept, but only with an O(1) fingerprint.** Hashing all 85 MB costs more than the ~60 ms it saves; grouping by (length, first and last eight bytes) and comparing the contents only inside a group costs < 0.2 ms and saves the 60 ms. |
| **"All of" literals** for the basic-auth rule (`"://"` *and* `"@"`) | **Neutral** (53 ms vs 54 ms). Kept out - it made the literal set fragile (see §5.3). |
| **`regex::RegexSet` instead of `aho-corasick`** for the prefilter | **Rejected.** The SIMD matcher is 2.7× faster on one large file (2.2 vs 0.83 GB/s), but a repository has many small files, and every set needs its own iterator: the security phase went from 54 ms to **84 ms** (8 sets) or **91 ms** (1 set). |
| **Parallel directory walk** (`WalkBuilder::threads`) | **Rejected.** No measurable difference - the walk is syscall bound. |
| **A window filter in the security prefilter** (only run a rule when the text around its literal has the right shape) | **Kept, but it is not the win it looks like.** It cuts the number of candidate rules from 1 295 to 81 and the time of the rules from 72 ms to 55 ms (single threaded, ~6 ms in parallel) - but the prefilter itself (the aho-corasick pass over 65 MB) is 64 % of the security phase and is not affected by it. |
| **`regex` instead of `aho-corasick` for the prefilter** | **Not implemented (yet).** The `regex` crate's lazy DFA scans the corpus 2.2× faster (35 ms vs 98 ms single threaded, 5.7 ms vs 13.6 ms in parallel), but every match needs a hash lookup to find its rule, and the whole gain is ~6 ms of a 380 ms run. |
| **Skipping the `free()` of the result at the end of the CLI** (`mem::forget`) | **Kept, but it does not show up in the wall clock.** The `free()` itself costs 8 ms, but the operating system unmaps the address space either way, and that is the 49 ms that the process spends outside of `main()`. |
| **A hand written pre-tokeniser for the `o200k_base` pattern** | **Rejected for now.** It would remove ~70 % of the token counting, but the BPE merge and the rank table are private in `tiktoken-rs`, so the whole encoder would have to be reimplemented - and the token count has to match the reference exactly. See §8. |

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
| `compare-flags.ps1` | the **remaining flags**, 15 cases | git flags, `--instruction-file-path`, `--token-count-encoding`, `--config`, `--stdin`, `--split-output`, `--token-budget`, several target directories, odd file encodings, exit codes |

**All three currently pass 26/26, 8/8 and 15/15.** The last two suites did
not exist before: the old suite only compared stdout in `--stdout` mode, so
the report — and with it the *entire* metrics phase — was never verified, and
15 flags were never compared at all. They caught five real bugs (see §5.3).

The fixtures (`testdata-git/`, `testdata-config/`, `testdata-multi/`,
`testdata-encoding/`) are generated by `make-fixtures.ps1`; the git fixture
is a real repository and therefore not committed.

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
  implementations over 1 600 input pairs.
  The Unicode Collation Algorithm has **three** levels - the base letter
  (primary), the accent (secondary) and the case (tertiary) - so `åäö.ts`
  sorts with the `a` files and not behind the `z` files. The decomposition
  table was generated from the Unicode data. The reference implementation
  calls `localeCompare()` *without* a locale, so its result for accented
  characters depends on the locale of the machine (en-SE here); the root
  collation is used here, which is identical for the ASCII characters and
  for the base letters of the accented ones.
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
  The **wrapper** itself is counted as a whole (the reference counts it with
  `runTokenCount`, which does not split it), while the **output** of the slow
  path is counted in 200 000 character chunks (the reference counts it with
  `calculateOutputMetrics`, which does). The per-file counts are never
  chunked. Mixing those up changes the reported total.
  The `indexOf` of the reference finds the *first* occurrence of the content
  at or after the cursor - even when that is not the place where the file
  was rendered. That behaviour is reproduced on purpose.
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
* **The git change counts are only used when the *working directory* is the
  root of a work tree.** The reference checks for a `.git` entry in the
  working directory and collects the counts there — it does not look at the
  parent directories, although `git log` itself would find them. The diffs
  and the log use `git rev-parse` instead and do work in a subdirectory.
* **The path separators are normalised on every platform.** The reference
  replaces the Windows separator with the POSIX one *unconditionally*
  (`path.win32.sep` is a constant in Node) and then splits on the POSIX
  separator, so a file name that contains a backslash is split into two
  segments on Linux as well. The Rust implementation does the same, and
  `util/path.rs::backslashes_are_split_like_the_reference` locks it in.

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
* **The usage guidelines were missing a blank line** when an instruction file
  was used without a header text: the reference builds the text from a
  template with two conditional lines and trims the result, so the empty
  header line survives the trim.
* **With several target directories the file paths in the output were the
  root relative paths** instead of the display paths, so they were missing
  the root label ("alpha/one.txt" became "one.txt").
* **`--stdin` was accepted but ignored** — the whole current directory was
  packed instead of the files that were piped in. It now reads the paths from
  stdin, drops the comments and empty lines, resolves and de-duplicates them,
  and expands a directory into the files below it. **Known divergence:** on
  Windows the reference packs *nothing* in this mode, because it builds the
  include pattern from `path.relative()`, which returns a path with
  backslashes there, and escapes the backslash into the glob pattern. The
  Rust implementation packs the requested files.
* **The files were sorted by their git change count outside a work tree**,
  and the counts were collected in the target directory instead of the
  working directory.
* **The order of the files was not preserved while reading them**: zipping
  two parallel iterators produces an iterator that is not *indexed*, and
  collecting from it does not keep the order.
* **The token total of the slow path was wrong** (the file token counts were
  added to the count of the whole output): the reference implementation only
  sums the per-file counts on the fast path.
* **The wrapper was counted in chunks**, the reference implementation counts
  it as a whole - a wrapper of more than one megabyte reported a slightly
  different number.
* **The glob matcher compared bytes instead of characters**: a `?` matched a
  single byte, and a non-ASCII character in a pattern was truncated to its
  last byte. A file named `åäö.ts` was therefore never matched by
  `--include "???.ts"`.
* **The path sort put every accented character behind `z`**: the collation
  only had one level, so `åäö.ts` was listed after `zebra.txt` while the
  reference implementation lists it before. The Unicode Collation Algorithm
  has three levels (the base letter, the accent and the case).

---

## 6. Verification commands

```powershell
# unit + integration tests (64)
cd repomix-rs; cargo test --release

# output differential: 26 flag combinations vs npx repomix (byte comparison)
.\compare-all.ps1 -Dir C:\Users\ozett\Documents\Projects\Rustmix\testdata `
                  -RustExe C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe

# report differential: 8 flag combinations (token/char/file counts, security)
.\compare-metrics.ps1 -Dir C:\Users\ozett\Documents\Projects\Rustmix\testdata `
                      -RustExe C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe

# flag differential: 15 cases (git flags, --stdin, --split-output, ...)
.\compare-flags.ps1 -RustExe C:\Users\ozett\Documents\Projects\Rustmix\repomix-rs\target\release\repomix.exe

# the fixtures the suites need (the git fixture is a real repository)
.\make-fixtures.ps1

# benchmark (Rust vs npx). NOTE: npx must be invoked through cmd.exe.
.\bench.ps1 -Target C:\Users\ozett\AppData\Local\npm-cache\_npx\cad0f0e99bf115c9\node_modules -Runs 3

# the wall clock of both paths, with the phase breakdown
.\measure.ps1 -Runs 3

# how much of the run is the process start and the teardown
.\measure-teardown.ps1 -Runs 3

# the peak memory of the process
.\measure-memory.ps1 -Runs 3

# phase timings
$env:REPOMIX_TIMING="1"; repomix.exe --stdout 2>timing.txt > out.xml

# the time the allocator needs for the result (the CLI skips it)
$env:REPOMIX_TIMING_DROP="1"; repomix.exe --stdout 2>timing.txt > out.xml

# micro-benchmarks of the hot primitives
cargo build --release --features dev --bin bench_prims
.\target\release\bench_prims.exe <some-file>

# the token counting: the regex pre-tokenisation vs the BPE, and the
# different scheduling strategies
cargo build --release --features dev --bin bench_tok
.\target\release\bench_tok.exe <a directory>

# the secret scan: the prefilter vs the rules
cargo build --release --features dev --bin bench_sec
.\target\release\bench_sec.exe <a directory>

# the collation of the path sort, next to the reference implementation
cargo build --release --features dev --bin collcmp
.\target\release\collcmp.exe

# per-rule regex compile cost
cargo build --release --features dev --bin regexbench
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
memchr 2.7 (new: the substring search of the wrapper, the XML escaping
             and the markdown delimiter)
arboard 3.7            anyhow  thiserror  log 0.4  env_logger 0.11
unicode-width 0.2
```

Release profile: `opt-level=3, lto="fat", codegen-units=1, panic="abort", strip=true`.

**The binary is statically linked.** `repomix-rs/.cargo/config.toml` sets
`-C target-feature=+crt-static` and compiles the C code of the tree-sitter
grammars with `/MT`, so the executable only imports the Windows system
libraries (`kernel32`, `advapi32`, `ntdll`, `user32`, `userenv`) and needs
neither the Visual C++ redistributable nor a Node runtime. **Cargo reads
`.cargo/config.toml` from the *working directory*, so the build has to run
inside the crate directory** - `install.ps1` and `cargo install` both do.
The static flags are scoped to the MSVC target, so the Linux and macOS
builds are not affected.

**Dependency policy:** adding a small, well-maintained crate is acceptable
(`aho-corasick`, `memchr`, `rayon` are already in the tree transitively).
Avoid anything that would change observable behaviour.

---

## 8. Known open issues / risks

* **The token counting is the remaining bottleneck of the report path.**
  ~700 ms of the 930 ms of a default run is the BPE of 65 MB. 70 % of that
  is the `fancy_regex` pre-tokenisation of the `o200k_base` pattern inside
  `tiktoken-rs` (the crate's own source says "most of the time is spent in
  regex"). A hand written pre-tokeniser would remove it, but the BPE merge
  and the rank table are private in `tiktoken-rs`, so the whole encoder
  would have to be reimplemented (including the `o200k_base.tiktoken`
  asset) - and the token count has to match the reference exactly. The
  measurements for it are in `src/bin/bench_tok.rs`.
* **The directory walk (78 ms) is syscall bound.** The `ignore` crate's
  parallel walker made no difference. A hand-written walker that avoids the
  per-entry `stat` (by using `read_dir` + `file_type`) might, but that would
  mean reimplementing `.gitignore` handling.
* **`util/glob.rs` is a hand-written backtracking matcher.** It is correct for
  the cases that matter, but it is O(patterns × path segments) per file. It
  now only accounts for 5.5 ms of the 83 ms search phase (the cheap
  necessary conditions of §4.3 skip most of the patterns), so it is *not*
  worth optimising further. `globset` cannot be swapped in blindly: its `**`
  semantics differ from picomatch's.
* **The `aho-corasick` prefilter is a correctness-critical optimisation.** It
  relies on every rule having a *necessary* literal. Four unit tests guard
  this: `prefilter_never_hides_a_matching_rule` (for every rule and every
  sample, a rule that matches must be in the prefilter's mask),
  `every_match_contains_one_of_its_literals` (every match contains one of the
  rule's literals), `no_literal_hides_another_one` (no literal may occur
  inside another one - aho-corasick's leftmost-first semantics report the
  *shortest* match and would hide the longer one) and
  `the_guard_never_hides_a_matching_rule` (the shape check around a literal
  must not hide a rule that matches). **Any new rule must come with a
  literal and a sample.**
* **The glob matcher compares characters, not bytes.** A `?` matches one
  character and a literal character in a pattern is compared with its UTF-8
  encoding, like the `picomatch` of the reference implementation. The
  collation of the path sort is a three level approximation of the ICU root
  collation; the reference implementation uses `localeCompare()` *without* a
  locale, so its result for accented characters depends on the locale of the
  machine.
* **The metrics phase is skipped when its result is not consumed**
  (`--stdout`, `--quiet`, no `--token-budget`). The library API keeps
  `Config::need_metrics = true` by default, so `pack()` still returns full
  metrics for library users.

* `panic = "abort"` means any panic kills the process — there is no error
  recovery. Two panics were found and fixed in 2026-10-21; more may lurk.
* `core/comments.rs` diverges from `strip-comments` in edge cases (accepted).
* **Portability: audited, but not exercised on another platform.** There is
  no `cfg(windows)` or `cfg(unix)` code in the crate at all, every file and
  process operation goes through the portable `std` API, and the dead
  `windows-sys` dependency was removed. A full `cargo check --target
  x86_64-unknown-linux-gnu` cannot run here: the tree-sitter grammars are
  compiled from C and there is no Linux cross compiler (no WSL, no Docker).
  The known runtime difference is the clipboard: `arboard` needs X11 or
  Wayland on Linux, and the error is propagated like any other.
* The `ignore` crate's `is_git_repository` probe was removed from the git
  helpers (it doubled the number of process spawns and `git log`/`git diff`
  fail in exactly the same situations).
* **Content de-duplication in the metrics phase is implemented** (with the
  O(1) fingerprint, see §4.3). A `node_modules` tree contains many identical
  files (`LICENSE`, repeated `.d.ts`), and each copy used to be tokenised
  separately.

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
