# yamaa for R <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=workflow_dispatch)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml?query=branch%3Amain+event%3Aworkflow_dispatch)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

## Overview

yamaa is an R package to build SDTM and ADaM datasets from yamaa
specifications, check those specifications, and render Define-XML. The package
is designed with these principles:

- One engine: R and Python run the same Rust engine, so the same specification
  and inputs produce the same datasets, and no Python runtime is needed.
- Explicit publication: runs build in memory, and files are written only when
  you call `save()`.
- Plain values: data frames hold doubles, character, `Date` and UTC `POSIXct`
  columns, and a value R cannot hold exactly raises an error instead of
  changing.
- One issues table: every finding is one row of the `issues` data frame.

```r
library(yamaa)

project <- yamaa_project("project.yaml")
run <- project$spec("DM")$run()      # one dataset, built in memory
run$output; run$issues; run$report
run$save()                           # write the dataset and its run report
project$run("adam")$save()           # every ADaM dataset
```

The [R and Python API](https://elong0527.github.io/yamaa/articles/api/) page
describes every call, the
[introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the
model in five minutes, and the
[rules](https://elong0527.github.io/yamaa/articles/rules/) define what every
specification item means.

## Installation

The package is distributed as a source tarball attached to each
[GitHub release](https://github.com/elong0527/yamaa/releases). Download
`yamaa_0.4.0.tar.gz` from the selected release, then install it:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
```

Installation compiles the Rust engine, so it needs R 4.6.1, Rust 1.90 or later
with Cargo, a C compiler and GNU make. Under rustup, the archive's
`rust-toolchain.toml` selects the release toolchain, Rust 1.99.1, in rustup's
minimal profile without Clippy or rustfmt. On Windows,
also install Rtools45 and the `x86_64-pc-windows-gnu` target on that
toolchain: `rustup toolchain install 1.99.1 --target x86_64-pc-windows-gnu`. A
target added to another toolchain is not used. Cargo downloads the locked Rust
dependencies that are not already in its cache. The
[release guidance](../../docs/release.md#supported-platforms) lists the
supported platforms.

The `R/yamaa/` directory lacks the shared Rust sources, so it cannot be
installed directly, for example with `remotes::install_github()`. From a
repository checkout, `make test-r` builds and checks a fresh source package;
see the [contributor guide](../../CONTRIBUTING.md).

## What the source package contains

`tools/staging/stage_r_package.py` assembles the standalone package before
`R CMD build` creates the release archive. The archive contains:

| Location | Contents | Installed for users? |
| --- | --- | --- |
| `R/`, `man/`, `NAMESPACE`, `DESCRIPTION` | R facade, help, native registration and package metadata | Yes; R converts code and help to its installed formats |
| `src/` | C entry point, build rules, the shared Rust workspace, Cargo lock and toolchain declaration, native test inputs, vendored YAML parser and ReadStat C sources | Compiled into `libs/yamaa.so` or `libs/yamaa.dll`; sources are not ordinarily installed |
| `inst/` | License notices and the specification, grammar, project, Define-XML and host test fixtures | Yes; copied to the package root |
| `tests/` | Installed-host regression scripts | Used by `R CMD check`; not ordinarily installed |

All five Rust workspace members are retained to preserve the canonical Cargo
manifest and lock, including the Python binding's source. Building `yamaa-r`
does not compile that Python binding and does not require Python. The package
does not bundle prebuilt native libraries or the complete registry dependency
sources: Cargo downloads missing locked crates during installation. The local
vendored parser and ReadStat sources are only part of that dependency graph.

The R runtime imports `utils`, which ships with R. `arrow` and `jsonlite` are
suggested packages used by the regression scripts to inspect saved files.
`yamaabenchmarks` is not required to build, install, use or check the R
package. Its packaged project tests use base R callables such as
`base::identity`, `base::signif` and `base::ifelse`.
The original benchmark projects still use their separate helper package in
repository qualification, outside the source package's test dependency graph.

## CRAN preparation

A successful repository check is not a CRAN acceptance decision. Before a
submission, run `R CMD check --as-cran` on the built tarball with current R
release and R-devel, and validate Windows and macOS as well as Linux.

The current build still needs work against CRAN's
[Rust guidance](https://cran.r-project.org/web/packages/using_rust.html):
check Cargo and rustc availability and versions before compiling, report the
compiler version, avoid automatic toolchain installation, limit Cargo to one
or two jobs, and prepare an appropriately compressed vendor bundle so builds
can run without downloading crates. The minimum Rust version must also be
verified against the full locked dependency graph, not only our workspace.

CRAN-style checks also flag the patch-level R requirement (`R >= 4.6.1`).
The planned R 4.4 floor and binding C API audit are tracked in
[#3058](https://github.com/elong0527/yamaa/issues/3058).
Inspect termination symbols linked
from Rust and native libraries, including `abort`, and establish whether they
can be reached before explaining any retained symbols in a submission.

Review the archive and installed library against the
[CRAN policies](https://cran.r-project.org/web/packages/policies.html),
particularly source size, copyright coverage of linked dependencies, build
time, and test time. `inst/` fixtures remain installed for every user; moving
test-only resources under `tests/` is a possible further size reduction.

## Learn more

- Package help: `?yamaa_spec`, `?yamaa_project` and `?yamaa_read_odm`.
- The [repository README](../../README.md#example) shows a complete
  specification, and the [benchmarks](https://elong0527.github.io/yamaa/benchmark/)
  provide many more.
- Language contracts: [project files](../../rules/profiles/project.md),
  [resource paths](../../rules/reference/files.md),
  [SAS sources](../../rules/reference/sas.md) and
  [submission documents](../../rules/README.md#rule-files).
- [Release guidance](../../docs/release.md): supported platforms,
  [resource limits](../../docs/release.md#resource-limits) and migration.
