# Prompt for the analysis model

> This prompt is **Part 1** of a single self-contained bundle. The same file
> also contains the engineering context (Part 2), the complete Rust source
> tree (Part 3) and the reference TypeScript implementation (Part 4).
> Upload or paste the whole bundle.

---

You are an expert systems programmer and performance engineer. I have built a
high-performance, drop-in replacement for the Repomix codebase packer in Rust
(`repomix-rs`). **Everything you need is in this file**: Part 2 is the
engineering context (architecture, measured performance, the byte-for-byte
output-compatibility contract, the verification commands, the open risks), Part
3 is the complete Rust source tree, and Part 4 is the original TypeScript
implementation, which is the ground truth for every observable behaviour.

Your job: **produce a concrete, prioritised performance optimization plan.**

Please read the source, not just the summary. The files that matter most are
listed in Part 2, section 2; the hot ones are:

- `src/core/packager.rs` — the pipeline
- `src/core/metrics.rs` — token counting (64 % of runtime)
- `src/core/security.rs` — the 30 secret-detection rules
- `src/core/search.rs` + `src/util/glob.rs` — the file walk and glob matcher
- `src/output/*.rs` — the formatters

For each proposed change I need:

1. **Expected speedup**, with the reasoning (what work is actually removed).
   Anchor it to the measured phase timings in Part 2, section 4.
2. **A concrete implementation sketch** — function signatures, which file, which
   data structure. Not "use a faster hashmap"; tell me exactly what to build.
3. **Correctness risk**, specifically against the output-compatibility contract
   in Part 2, section 5 and the 26-case differential suite in section 6. Say
   plainly if a change could alter a single output byte.
4. **Anything you think is currently wrong or fragile** in the hot paths.

Constraints I must not violate:

- Output must stay **byte-identical** to `npx repomix` for all 26 tested flag
  combinations (Part 2, section 5). The `--remove-comments` divergence is
  deliberate and must not be "fixed".
- No behaviour changes, no caching that changes output, no new user-visible
  flags.
- A single statically-linked binary; adding a small, well-maintained crate is
  fine (`aho-corasick` and `memchr` are already in the tree transitively).
- Cold start must stay under 15 ms (currently 6.4–7.7 ms).

Priorities, in order: **(a) token counting, (b) secret scan, (c) search/glob
matching, (d) render, (e) cold start.**

Finish with a **ranked "do this first" list of at most 5 items**, each with a
one-line expected win. If you think a measurement is missing, name it exactly
and give me the command to produce it using the tools in Part 2, section 6 — I
would rather measure than guess.
