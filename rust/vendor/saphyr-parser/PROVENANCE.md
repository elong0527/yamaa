# saphyr-parser 0.2.1

Source: the crates.io `saphyr-parser` 0.2.1 release, corresponding to
[saphyr v0.2.1](https://github.com/saphyr-rs/saphyr/tree/1583482c4f08f3c14caea0e2f1a8526950fb0eff/parser).
The upstream release commit is `1583482c4f08f3c14caea0e2f1a8526950fb0eff`.

All `src` files are the released bytes except `src/scanner.rs`. Its complete
change is recorded in `compact-flow-pairs.patch`, against the release's
`parser/src/scanner.rs`: mapping-start and implicit-pair state follow each flow
collection level, so a comma in a nested mapping does not close the outer
compact pair. No parser API or dependency was changed. This fixes yamaa #2196.

`Cargo.toml` retains the released library identity, features and dependency
requirements, with upstream tools, benchmarks and development dependencies
omitted. `tests/compact_flow_pairs.rs` adds independent expected event streams
and string/iterator parity controls. Run them with
`cargo test --locked -p saphyr-parser --test compact_flow_pairs` from
`rust/` (the dependency is excluded from the five owned workspace members).

`README.md`, `LICENSE` and all four `.licenses` files retain the release's
original bytes. Redistributions must include licenses from both Chen Yuheng
and Ethiraric. The Python wheel and installed R package include both MIT
notices in `LICENSE-YAML.txt`; source packages retain both licensing options.

The crates.io patch uses local release sources so locked native rebuilds do
not depend on a personal Git fork. Remove this patch after a released upstream
version passes the same decoder, host and source-package controls.
