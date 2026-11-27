# Fuzz targets

These [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets fuzz the
readers that parse untrusted input, and the engine surfaces behind them. Each
target feeds its input to an entry point the hosts use, and that entry point
applies the fixed `Limits` itself.

| Target | Input | Entry point |
| --- | --- | --- |
| `parquet` | A Parquet file | The shared source decode, as an ordinary and an ODM input: framing, footer and page admission, then the native decoder. Read by partition, a run of whole row groups at a time, it must give the whole read's table or finding. |
| `sas7bdat` | A SAS7BDAT file | The shared source decode through the vendored ReadStat reader: undeclared, as ODM, and with numeric fields declared `int`, `date` and `datetime`. |
| `xport` | A SAS transport file | As `sas7bdat`, after the transport member admission. |
| `csv` | A CSV file | The shared source decode: undeclared, as ODM, with a declared byte-order mark, and with every field declared each typed column type. The mark followed by the input must read as the input alone, unless the input starts with a mark. Read by partition, the record scan then one range of records at a time, each must give the whole read's table or finding. |
| `specification` | A YAML specification | Preparation and compilation as a host check runs them. The engine admits the shipped schema closure once per process. |
| `dataset_json` | A published CSV artifact | The Dataset-JSON writer over the artifact, admitted as a submission build reads it, in both representations. The output must parse as JSON holding every cell: one object, or a header line and one row per line. Each file must then read back through the shared source decode, as a `.json` or `.ndjson` input, as the table it was written from. |
| `odm_xml` | An ODM XML file, or a plain or gzip TAR archive holding one | The ODM XML reader that `read_odm` calls in both hosts, with its default options; then the shared source decode of a specification's `.xml` input, which reads the bytes as XML alone, as an ordinary and an ODM input. |
| `fixed_width` | A positional reading's layout in the first bytes, then the text | The shared source decode of a csv input that declares `fixed_width` (CSV-18): undeclared, then with every field declared each typed column type, each read by partition too, as `csv` reads them. |
| `portable_regex` | A pattern, then a NUL byte and a subject | The regex subset's compilation, then its searches: the restarted search and the checkpointed backward scan of `search_all` must agree, for the subject and for it repeated past `RESTART_BYTES`, and their first match is the one `search` and `is_match` report. |
| `packaging_lock` | A packaging lock, then a NUL byte and an installed version | The lock classification of every project preparation, the renv reader, and uv lock verification with its PEP 440 and PEP 508 readers, for a CPython 3.12 installation on Linux whose distributions report the given version. |
| `arrow_stream` | Bytes that build a foreign Arrow C stream | Both imports behind the hosts' tables: the result table `Table::import` restores and an input frame. The stream keeps the producer contract the interface leaves to the producer; the input chooses its schema, nesting, batches, slices, validity, text bytes and callback faults. Each import must read the stream with every buffer under-aligned as it reads it aligned. |
| `study` | A study directory: `spec.yaml`, then each other file as a NUL byte, its path, a line feed and its bytes | The static check, then preparation and a run, on the file route through an in-memory resolver. A `project.yaml` is the project of both, so its layers, function, codelist and analysis results files and lock are read as their own file types. The check is a Python host's, the run an R host's, through the empty host-functions port. |

The shared source decode is the one both host routes call for held input bytes
(`decoder::Decoder`). Untrusted bytes reach the Dataset-JSON writer as a
published artifact, so the `dataset_json` target fuzzes that path, and the
Dataset-JSON source profiles' reader over the files it writes. The ODM XML reader takes a path, so the `odm_xml` target writes
each input to one scratch file in the system's temporary directory first.

A run fails on a crash, a sanitizer report, a panic, a timeout or a memory
blowup. libFuzzer's panic hook aborts on every panic it sees. The Parquet
reader contains a panic in the third-party decoder by design and keeps it from
that hook, so such an input stays a malformed source, as the hosts report it.
The SAS targets also fail when the reader returns an internal error for input
bytes, `dataset_json` when a written file does not read back, and `odm_xml`
when the reader cannot read the file it was given or returns an internal error,
or when an `.xml` input is refused for anything but its document. `fixed_width`
fails as `csv` does when a read by partition differs from the whole read,
`portable_regex` when the two scans or the searches disagree,
`packaging_lock` when a lock classified as renv does not read as one, and
`arrow_stream` when an import reads the displaced stream otherwise.

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
directories is committed. CI keeps each target's corpus in the Actions cache
instead: every run restores the corpus main last saved, and a passing run on
main shrinks it with `python -m tools.ci.fuzz minimize <target>` and saves it
for the next night. `python -m tools.ci.fuzz build` builds every target,
and `python -m tools.ci.fuzz seeds <target> <directory>` writes one target's
seeds. A built target runs the input files named as its arguments once each:

```sh
rust/fuzz/target/x86_64-unknown-linux-gnu/release/xport rust/fuzz/artifacts/xport/crash-<name>
```

The seeds are every committed file with the target's extension, `.yaml` files
for `specification`, `.csv` files for `dataset_json` and `fixed_width`, `.xml`
files for `odm_xml`, renv and uv locks for `packaging_lock`, `.parquet` files
for `arrow_stream` (any bytes build a stream), the regex conformance fixture
for `portable_regex` and each benchmark's `spec.yaml` for `study`, so new
fixtures seed the next run. Some targets derive further seeds from each
fixture. Each `odm_xml` seed is also written as the only member of a ustar, a
GNU and a PAX TAR archive and of a gzip-compressed one, so the archive reader
starts from valid headers. Each regex conformance case is written as its
pattern and subject. Each benchmark specification is written with its case's
directory, without its README, `benchmark.yaml` or `expected/`, leaving out a
file that holds a NUL byte. Nothing under `benchmarks/` is changed.

## Build

This directory is a Cargo workspace of its own, outside `rust/Cargo.toml`, so
the shipped dependency graph and the canonical `rust/Cargo.lock` stay
unchanged. The tool copies that lock here before each build: the engine builds
on the reviewed versions, and only the two fuzzing crates resolve, at the exact
versions `Cargo.toml` pins. The copy is not committed. The repository lint
(`make quick`) runs Clippy over the targets with the workspace toolchain, so a
change that breaks a target fails on its pull request.

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
A failing run on main opens a `main-red` issue, or comments on the open one,
and a person closes it once the crash is fixed: a later green run may simply
not find the same input again.
