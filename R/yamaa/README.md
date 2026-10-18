# yamaa for R <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

## Overview

yamaa is an R package to build SDTM and ADaM datasets from yamaa
specifications, check those specifications, and render Define-XML. The package
is designed with these principles:

- One engine: R and Python run the same Rust engine, so the same specification
  and inputs produce the same datasets, and no Python runtime is needed.
- Explicit publication: builds run in memory, and files are written only when
  you call `save()`.
- Exact values: 64-bit integers and text containing NUL are kept without loss,
  and a conversion that would lose a value raises an error.
- Structured findings: each finding is returned both as a row of the `issues`
  data frame and as a `diagnostics` record.

The [introduction](https://elong0527.github.io/yamaa/articles/intro/) explains
the model in five minutes, and the
[rules](https://elong0527.github.io/yamaa/articles/rules/) define what every
specification item means.

## Installation

The package is distributed as a source tarball attached to each
[GitHub release](https://github.com/elong0527/yamaa/releases). Download
`yamaa_0.4.0.tar.gz` from the selected release, then install it:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
```

Installation compiles the Rust engine, so it needs R 4.6.1, Rust 1.90.0 with
Cargo, a C compiler and GNU make. On Windows, also install Rtools45 and the
`x86_64-pc-windows-gnu` Rust target. Cargo downloads the locked Rust
dependencies that are not already in its cache. The
[release guidance](../../docs/release.md#supported-platforms) lists the
supported platforms.

The `R/yamaa/` directory lacks the shared Rust sources, so it cannot be
installed directly, for example with `remotes::install_github()`. From a
repository checkout, `make test-r` builds and checks a fresh source package;
see the [contributor guide](../../CONTRIBUTING.md).

## Highlighted features

- `yamaa_domain()` builds one dataset; `yamaa_check()` checks a specification
  without reading study data.
- `yamaa_define()` renders Define-XML, and Dataset-JSON on request, without
  running a specification.
- Inputs can be CSV, Parquet, SAS datasets (`.sas7bdat`) or SAS transport v5/v8
  files (`.xpt`); outputs are CSV or Parquet.
- Output data frames hold dates as `Date`, datetimes as UTC `POSIXct`, integer
  columns as `yamaa_int64_vector` (the full signed 64-bit range) and text
  containing NUL as `yamaa_utf8_vector`.
- With nanoarrow 0.8.0 or newer, `output_arrow`, `verification_log_arrow` and
  `warning_log_arrow` return Arrow streams that share the result's buffers
  instead of copying them.
- A `resolver` passed to `yamaa_domain()` or `yamaa_check()` reads
  specifications and inputs from, and saves outputs to, memory or your own
  storage instead of local files.
- `yamaa_resolve_specification()` records the composed specification, with the
  origin of every field, as versioned JSON.

## Simple example

```r
library(yamaa)

yamaa_check("spec.yaml")$issues     # Check the specification without data

result <- yamaa_domain("spec.yaml") # Build the dataset in memory
result$issues                       # Review the findings
result$output                       # Inspect the derived data frame
result$save()                       # Write the declared output files

define <- yamaa_define("environment.yaml", "sdtm")
define$save()                       # Write Define-XML
```

The [repository README](../../README.md#example) shows a complete
specification, and the [benchmarks](https://elong0527.github.io/yamaa/benchmark/)
provide many more.

## Learn more

- Package help: `?yamaa_domain`, `?yamaa_define`, `?yamaa_resource_resolver`,
  `?yamaa_resolve_specification` and `?int64_vector_conversions`.
- Behavior shared with Python:
  [host resources](../../python/README.md#host-resources),
  [diagnostics](../../python/README.md#diagnostics),
  [Arrow handoff](../../rust/README.md#arrow-host-handoff) and
  [resolved specification artifacts](../../rust/README.md#resolved-specification-artifact).
- Language contracts: [resource paths](../../rules/reference/files.md),
  [SAS sources](../../rules/reference/sas.md) and
  [submission documents](../../rules/README.md#rule-files).
- [Release guidance](../../docs/release.md): supported platforms,
  [resource limits](../../docs/release.md#resource-limits) and migration.
