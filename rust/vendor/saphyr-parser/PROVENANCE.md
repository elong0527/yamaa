# saphyr-parser 0.2.1, vendored as yamaa-saphyr-parser

Source: the crates.io `saphyr-parser` 0.2.1 release, corresponding to
[saphyr v0.2.1](https://github.com/saphyr-rs/saphyr/tree/1583482c4f08f3c14caea0e2f1a8526950fb0eff/parser).
The upstream release commit is `1583482c4f08f3c14caea0e2f1a8526950fb0eff`.

All `src` files are the released bytes except `src/scanner.rs` and
`src/parser.rs`. The complete `src/scanner.rs` change is recorded in
`compact-flow-pairs.patch`, against the release's `parser/src/scanner.rs`:
mapping-start and implicit-pair state follow each flow collection level, so a
comma in a nested mapping does not close the outer compact pair. No parser API
or dependency was changed. This fixes yamaa #2196. `src/parser.rs` differs only
in the hidden import line of its one doc example, which names the renamed
library.

`Cargo.toml` retains the released version, features and dependency
requirements, with upstream tools, benchmarks and development dependencies
omitted. It declares edition 2021 and `rust-version` 1.82.0, yamaa's minimum,
where the release declares edition 2024 and 1.85.0; the sources compile
unchanged under both. It names the package `yamaa-saphyr-parser` and the library
`yamaa_saphyr_parser`, so a graph that already holds a published
`saphyr-parser` gets a distinct crate rather than a second copy under the same
name: no duplicate package and no rustdoc output collision (#2825).
`rustfmt.toml` keeps the release's 2024 formatting style for these sources,
which `cargo fmt --all` in `rust/` formats as a path dependency.
`tests/compact_flow_pairs.rs` adds independent expected event streams and
string/iterator parity controls. Run them with
`cargo test --locked -p yamaa-saphyr-parser --test compact_flow_pairs` from
`rust/` (the crate is excluded from the five owned workspace members). CI also
runs them, directly and through the facade, from the outside workspace in
`rust/tests/embedder`.

All four files in `LICENSES` retain the release's original license bytes.
The license directory uses a visible name so R source-package checks admit
the complete notices; only license links in `README.md` and `LICENSE` were
updated for this relocation. Redistributions must include licenses from both
Chen Yuheng and Ethiraric. The Python wheel and installed R package include
both MIT notices in `LICENSE-YAML.txt`; source packages retain both licensing
options.

`yamaa-adapters` depends on this crate by path, requiring exactly version
0.2.1. A path dependency reaches every graph that builds the adapters,
including a project that embeds the facade by git or path; the crates.io
`[patch]` that 0.4.0 used applied only when yamaa's own workspace was the root
(#2929). The local release sources also keep locked native rebuilds
independent of a personal Git fork. Replace this crate after a released
upstream version passes the same decoder, host and source-package controls
(#2583).
