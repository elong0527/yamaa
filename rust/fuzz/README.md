# Fuzz targets

These [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets fuzz the
readers that parse untrusted input. Each target feeds its input to an entry
point the hosts use, and that entry point applies the fixed `Limits` itself.

| Target | Input | Entry point |
| --- | --- | --- |
| `parquet` | A Parquet file | The shared source decode, as an ordinary and an ODM input: framing, footer and page admission, then the native decoder. |
| `sas7bdat` | A SAS7BDAT file | The shared source decode through the vendored ReadStat reader: undeclared, as ODM, and with numeric fields declared `int`, `date` and `datetime`. |
| `xport` | A SAS transport file | As `sas7bdat`, after the transport member admission. |
| `csv` | A CSV file | The shared source decode: undeclared, as ODM, and with every field declared each typed column type. |
| `specification` | A YAML specification | Preparation and compilation as a host check runs them, with the shipped schema closure admitted once. |
| `dataset_json` | A published CSV artifact | The Dataset-JSON writer over the artifact, admitted as a submission build reads it. The output must parse as JSON holding every cell. |

The shared source decode is the one both host routes call for held input bytes
(`project_source_decoder::Decoder`). The engine writes Dataset-JSON but reads
none, so the `dataset_json` target fuzzes the path where untrusted bytes reach
that writer.

A run fails on a crash, a sanitizer report, a panic that escapes the entry
point, a timeout or a memory blowup. A panic the engine contains by design does
not fail it: the Parquet reader turns a panic in the third-party decoder into a
malformed source. The SAS targets also fail when the reader returns an internal
error, which it does only when a Rust callback contained a panic or the C
bridge reported an impossible status.

## Running locally

Run these from the repository root. They need rustup, clang and Python 3.12 or
later; `install` adds the pinned nightly toolchain and cargo-fuzz.

```sh
python -m tools.ci.fuzz install
python -m tools.ci.fuzz run parquet --seconds 300
```

`run` builds the targets, writes the seeds afresh to `rust/fuzz/seeds/<target>/`
and fuzzes for the given time. New inputs collect in
`rust/fuzz/corpus/<target>/` and carry over to the next local run; a failing
input is saved under `rust/fuzz/artifacts/<target>/`. None of these
directories is committed. `python -m tools.ci.fuzz build` builds every target,
and `python -m tools.ci.fuzz seeds <target> <directory>` writes one target's
seeds. A built target runs the input files named as its arguments once each:

```sh
rust/fuzz/target/x86_64-unknown-linux-gnu/release/xport rust/fuzz/artifacts/xport/crash-<name>
```

The seeds are every committed file with the target's extension, `.yaml` files
for `specification` and `.csv` files for `dataset_json`, so new fixtures seed
the next run. Nothing under `benchmarks/` is changed.

## Build

This directory is a Cargo workspace of its own, outside `rust/Cargo.toml`, so
the shipped dependency graph and the canonical `rust/Cargo.lock` stay
unchanged. The tool copies that lock here before each build: the engine builds
on the reviewed versions, and only the fuzzing crates resolve fresh. The copy
is not committed.

cargo-fuzz builds the Rust code with AddressSanitizer, coverage
instrumentation, debug assertions and overflow checks. The tool also sets `CC`
and `CFLAGS`, which the `cc` crate reads, so the C that this build compiles
carries AddressSanitizer, UndefinedBehaviorSanitizer with every report fatal,
and libFuzzer's coverage. That C is the vendored ReadStat reader and the
Zstandard library behind the Parquet codec. No other build sets those
variables, so normal builds compile the same C unchanged.

## Regression inputs

Fix a crash in the reader, with a unit test in the owning crate that
reproduces it. Then commit the failing input as text under
`rust/fuzz/regressions/<target>/<name>.hex`: hexadecimal digits, with lines
starting with `#` as comments. Every run decodes these inputs into the seeds
and replays them first. No binary input is committed.

## CI

The `Fuzz` workflow runs, for three minutes each, the targets whose readers a
pull request changes, as `python -m tools.ci.fuzz select` lists them. The
nightly schedule and a dispatch run every target for thirty minutes, or for the
dispatched time. A failing job uploads the reproducing input as an artifact.
