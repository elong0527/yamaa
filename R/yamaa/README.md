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
compiler, and Cargo and rustc 1.82 or later, from [rustup](https://rustup.rs/)
or from system packages that are that recent. The build uses your default
Rust toolchain, found on the PATH or in `~/.cargo/bin`, and rustup installs
none; installation stops first, with advice, when Cargo or rustc is missing
or older. On Windows, also install Rtools45 and the `x86_64-pc-windows-gnu`
target: `rustup target add x86_64-pc-windows-gnu`. The archive carries the
locked Rust crates, so installation needs no network; it compiles with two
jobs and writes nothing outside its build directory and the temporary
directory. The [release guidance](../../docs/release.md#supported-platforms)
lists the supported platforms.

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
| `src/` | C entry point, build rules, and in `rust/` the engine and R binding crates with their workspace manifest and trimmed Cargo lock, the locked crates.io crates in `rust/vendor.tar.xz` with their source replacement, the vendored YAML parser and ReadStat C sources | Compiled into `libs/yamaa.so` or `libs/yamaa.dll`; sources are not ordinarily installed |
| `inst/` | License notices, `AUTHORS`, and the specification, grammar, project, Define-XML and host test fixtures | Yes; copied to the package root |
| `tests/` | Installed-host regression scripts | Used by `R CMD check`; not ordinarily installed |
| `configure`, `configure.win`, `cleanup`, `cleanup.win` | Run `tools/configure.R` to find and check Cargo and rustc and write `src/Makevars` from `src/Makevars.in` (or the Windows pair); remove what it writes | No |
| `tools/` | `configure.R`, and `vendor_crates.py`, the script that wrote `src/rust/vendor.tar.xz` | No |

`src/rust` holds the engine crates and the R binding crate, without their
tests, and a Cargo lock trimmed from the repository's to the crates that they
build, so the package carries neither the Python binding nor its
dependencies. The package bundles no prebuilt native libraries and no Rust
toolchain file, and it carries the source of every locked crate, so
installation needs no network: the crates.io crates in
`src/rust/vendor.tar.xz`, written by `cargo vendor` when the package is
staged, and the vendored parser and ReadStat sources beside the workspace. The
archive leaves out the prebuilt WebAssembly objects of `wit-bindgen`, a crate
that only WebAssembly targets build, and the toolchain files that some crates
publish. `inst/AUTHORS` names every bundled crate's authors and license.

The R runtime imports `utils`, which ships with R. `arrow` and `jsonlite` are
suggested packages: the regression scripts read their JSON truth with
`jsonlite` and inspect saved files with both. As "Writing R Extensions" asks,
the scripts use them only where they are installed, and skip, with a
message, the parts that need a missing one.
`yamaabenchmarks` is not required to build, install, use or check the R
package. Its packaged project tests use base R callables such as
`base::identity`, `base::signif` and `base::ifelse`.
The original benchmark projects still use their separate helper package in
repository qualification, outside the source package's test dependency graph.

## CRAN preparation

A successful repository check is not a CRAN acceptance decision. Before a
submission, run `R CMD check --as-cran` on the built tarball with current R
release and R-devel, and validate Windows and macOS as well as Linux. Native
qualification's `native-cran` jobs do so on every run, for the tarball that a
release publishes: with R-devel, R-release and R 4.4 on all three platforms
and Rust 1.82, with R-devel on Linux and none of the suggested packages, as
CRAN's noSuggests check runs, and in R-hub's Fedora `gcc16` container with
Fedora's Rust.

The build follows CRAN's
[Rust guidance](https://cran.r-project.org/web/packages/using_rust.html):
`configure` checks that Cargo and rustc are present and at least the
workspace's `rust-version`, reports their versions before compiling and never
installs a toolchain, and the Makevars run `cargo build --offline -j 2` from
the bundled crates with a Cargo home inside the build tree. Rust 1.82, the
minimum, builds the full locked dependency graph, not only our workspace.
`cran-comments.md` explains the notes that a submission reports, and
`make test-r` fails on any other.

The package declares `R (>= 4.4.0)` after an audit of the R C API that the
binding calls ([#3058](https://github.com/elong0527/yamaa/issues/3058)).
The shared object imports `abort` through the Rust standard library, which
R's compiled-code check does not attribute to the package's own code. It is
reached only on fatal conditions of the Rust runtime, such as a failed
allocation or a panic while another panic unwinds; every call from R catches
an ordinary panic and returns it as an R error.

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
