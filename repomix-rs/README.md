# repomix-rs

The Rust implementation of **Rustmix** - a from-scratch rewrite of
[Repomix](https://github.com/yamadashy/repomix) that produces byte-identical
output and runs about 12x faster.

See the [project README](../README.md) for the installation, the usage and
the performance numbers, and [`../CONTEXT.md`](../CONTEXT.md) for the
architecture and the output-compatibility contract.

## Building

```bash
cargo build --release
```

The build has to run inside this directory: cargo reads
`.cargo/config.toml` from the working directory, and that file is what links
the C runtime statically on Windows.

## Testing

```bash
cargo test --release
```

The differential test suites that compare against the original
implementation live in the parent directory.

## License

MIT
