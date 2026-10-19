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

Installation compiles the Rust engine, so it needs R 4.4.0 or later, a C
compiler, and Cargo and rustc 1.90 or later, from [rustup](https://rustup.rs/)
or from system packages that are that recent. The build uses your default
Rust toolchain, and rustup installs none. On
Windows, also install Rtools45 and the `x86_64-pc-windows-gnu` target:
`rustup target add x86_64-pc-windows-gnu`. The archive carries the locked
Rust crates, so installation needs no network; it compiles with two jobs and
writes nothing outside its build directory and the temporary directory. The
[release guidance](../../docs/release.md#supported-platforms) lists the
supported platforms.

The `R/yamaa/` directory lacks the shared Rust sources, so it cannot be
installed directly, for example with `remotes::install_github()`. From a
repository checkout, `make test-r` builds and checks a fresh source package;
see the [contributor guide](../../CONTRIBUTING.md).

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
