# yamaa for R <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=workflow_dispatch)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml?query=branch%3Amain+event%3Aworkflow_dispatch)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

## Overview

yamaa is an R package to build SDTM and ADaM datasets from yamaa
specifications, check those specifications, and render Define-XML. The package
is designed with these principles:

- One engine: R, Python and the `yamaa` command line run the same Rust engine,
  so the same specification and inputs produce the same datasets, and no
  Python runtime is needed.
- Explicit publication: runs build in memory, and files are written only when
  you call `save()`, which returns the paths it wrote.
- Plain values: data frames hold doubles, character, `Date` and UTC `POSIXct`
  columns, and a value R cannot hold exactly raises an error instead of
  changing.
- One issues table: every finding is one row of the `issues` data frame, with
  its severity, condition, rule, spec path and message.

```r
library(yamaa)

project <- yamaa_project("project.yaml")
run <- project$spec("DM")$run()      # one dataset, built in memory
run$output; run$issues; run$report
run$status                           # "succeeded" or "failed"
paths <- run$save()                  # write the dataset and its run report
project$run("adam")$save()           # every ADaM dataset
```

`save()` returns, invisibly, the absolute paths of the files it wrote, in the
order it wrote them.

The
[R, Python and command-line API](https://elong0527.github.io/yamaa/articles/api/)
page describes every call, the
[introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the
model in five minutes, and the
[rules](https://elong0527.github.io/yamaa/articles/rules/) define what every
specification item means.

## Installation

From 0.6.0 on, each
[GitHub release](https://github.com/elong0527/yamaa/releases) carries binary
packages for Windows x64 and macOS arm64, built for R-release and R-oldrel,
today R 4.6 and R 4.5. They install without Rust, Rtools or a network, and
the macOS ones need macOS 11 or later. A binary package works only with the
R series that ends its file name, so download the one for your platform and
R from the selected release, and install it with the line for that file:

```r
# Windows x64
install.packages("yamaa_<version>-4.6.zip", repos = NULL)  # R 4.6
install.packages("yamaa_<version>-4.5.zip", repos = NULL)  # R 4.5
# macOS arm64
install.packages("yamaa_<version>-4.6.tgz", repos = NULL)  # R 4.6
install.packages("yamaa_<version>-4.5.tgz", repos = NULL)  # R 4.5
```

On Linux, with any other R series, such as R 4.2, and for 0.5.0 and
earlier, install the source package `yamaa_<version>.tar.gz` from the same
release:

```r
install.packages("yamaa_<version>.tar.gz", repos = NULL, type = "source")
```

0.7.0 is not submitted to CRAN, so install it from the GitHub release
(#4064 D304). `install.packages("yamaa")` installs the older version that
CRAN serves.

Installing the source package compiles the Rust engine, so it needs R 4.2.0
or later, a C compiler, and Cargo and rustc 1.82 or later, from
[rustup](https://rustup.rs/) or from system packages that are that recent.
The build uses your default Rust toolchain, found on the PATH or in
`~/.cargo/bin`, and rustup installs none; installation stops first, with
advice, when Cargo or rustc is missing or older. On Windows, also install
the Rtools version for your R series (Rtools42 for R 4.2) and the
`x86_64-pc-windows-gnu` target:
`rustup target add x86_64-pc-windows-gnu`. The archive carries the locked
Rust crates, so installation needs no network; it compiles with two jobs and
writes nothing outside its build directory and the temporary directory. A
Linux machine without such a Rust can take the offline toolchain route:
install Rust from its standalone installer, carried in, without rustup or a
network.
[Installing in a restricted environment](../../docs/articles/restricted-environments.md)
gives its steps, and those for installing on a machine without internet
access from an internal CRAN mirror or from files carried in. The
[release notes](https://github.com/elong0527/yamaa/releases/latest) list the
supported platforms.

The `R/yamaa/` directory lacks the shared Rust sources, so it cannot be
installed directly, for example with `remotes::install_github()`. From a
repository checkout, `make test-r` builds and checks a fresh source package;
see the [contributor guide](../../CONTRIBUTING.md).

For a small self-contained example, open `vignette("yamaa-cran", package =
"yamaa")`. The full documentation remains on the site.

## What the source package contains

`tools/staging/stage_r_package.py` assembles the standalone package before
`R CMD build` creates the release archive. The archive contains:

| Location | Contents | Installed for users? |
| --- | --- | --- |
| `R/`, `man/`, `NAMESPACE`, `DESCRIPTION` | R facade, help, native registration and package metadata | Yes; R converts code and help to its installed formats |
| `src/` | C entry point, build rules, and in `rust/` the engine, boundary and R binding crates with their workspace manifest and trimmed Cargo lock, the locked crates.io crates in `rust/vendor.tar.xz` with their source replacement, the vendored YAML parser and ReadStat C sources | Compiled into `libs/yamaa.so` or `libs/yamaa.dll`; sources are not ordinarily installed |
| `inst/` | License notices, the component inventory `COMPONENTS.tsv` and `AUTHORS` | Yes; copied to the package root |
| `tests/` | A base-R smoke script and its small project study; it runs the embedded format selftest and checks public R calls, callbacks and saved bytes | Used by `R CMD check`; not ordinarily installed |
| `vignettes/` | A minimal R Markdown introduction linking the full documentation | R builds its HTML and source into `inst/doc/` for installation |
| `configure`, `configure.win`, `cleanup`, `cleanup.win` | Run `tools/configure.R` to find and check Cargo and rustc, write `src/Makevars` from `src/Makevars.in` (or the Windows pair) and add the Rust standard library that rustc links to `inst/COMPONENTS.tsv`; remove what it writes | No |
| `tools/` | `configure.R`, and `vendor_crates.py` with its `vendor_exclusions.toml` policy, which wrote `src/rust/vendor.tar.xz` | No |

The full regression suites and their inputs stay in the repository. `make
test-r`, pull-request CI and native qualification run them separately against
the installed package; reducing CRAN's test payload does not reduce that
release gate. The smoke script needs no suggested packages.

`src/rust` holds the engine crates, the boundary crate and the R binding
crate, without their tests, and a Cargo lock trimmed from the repository's to
the crates that they build, so the package carries neither the Python binding nor its
dependencies. The package bundles no prebuilt native libraries and no Rust
toolchain file, and it carries the source of every locked crate that a
supported platform compiles, so installation needs no network: the crates.io
crates in `src/rust/vendor.tar.xz`, written by `cargo vendor` when the package
is staged, and the vendored parser and ReadStat sources beside the workspace.
Each locked crate that no supported platform compiles, such as `r-efi`,
`windows-core` and the WebAssembly crates, keeps only its manifest, which
Cargo reads to resolve the lock. The archive also leaves out the toolchain
files that some crates publish, and zstd's text of the license that it offers
beside BSD-3-Clause, which yamaa relies on. `inst/AUTHORS` names every bundled
crate's authors and license.

The version-specific vendoring policy also omits audited upstream tests,
benchmarks, standalone examples and CI files. It keeps licenses, production
inputs and examples that Rust embeds while compiling, including winnow's.
New dependency versions keep their development files until reviewed.

The R runtime imports `utils`, which ships with R. `arrow` and `jsonlite` are
suggested packages: the regression scripts read their JSON truth with
`jsonlite` and inspect saved files with both. As "Writing R Extensions" asks,
the scripts use them only where they are installed, and skip, with a
message, the parts that need a missing one.
`knitr` and `rmarkdown`, with Pandoc, build the introductory vignette; they
are not runtime requirements. Source-package builders need them before
running `R CMD build`.
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
release publishes: with R-devel, R-release and R 4.2.0 on all three platforms
and Rust 1.82, with R-devel on Linux and none of the suggested packages, as
CRAN's noSuggests check runs, with R-devel on Linux and the PDF and HTML
manuals, which CRAN's incoming checks build and validate, in R-hub's Fedora
`gcc16` container with Fedora's Rust, in R-hub's `gcc-asan` container with
Fedora's Rust and its `clang-asan` container with Rust 1.82, and in R-hub's
`valgrind` container with Fedora's Rust. In the two sanitizer containers
the package's C, the bundled ReadStat and zstd sources included, is compiled
with AddressSanitizer and UndefinedBehaviorSanitizer, as in CRAN's
additional checks, and any sanitizer report fails the check. In the valgrind
container the examples and tests run under valgrind's memcheck, as in CRAN's
valgrind check, and any error that it counts fails the check.

The build follows CRAN's
[Rust guidance](https://cran.r-project.org/web/packages/using_rust.html):
`configure` checks that Cargo and rustc are present and at least the
workspace's `rust-version`, reports their versions before compiling and never
installs a toolchain, and the Makevars run `cargo build --offline -j 2` from
the bundled crates with a Cargo home inside the build tree. They give Cargo
R's C compiler, so the C sources that crates compile build with the compiler
of the package's own C, sanitizer flags included. Rust 1.82, the
minimum, builds the full locked dependency graph, not only our workspace.
`cran-comments.md` explains the notes that a submission reports, and
`make test-r` fails on any other.

The package declares `R (>= 4.2.0)`, qualified on the oldest admitted patch
with the minimum Rust ([#3561](https://github.com/elong0527/yamaa/issues/3561)).
The shared object imports `abort` through the Rust standard library, which
R's compiled-code check does not attribute to the package's own code. It is
reached only on fatal conditions of the Rust runtime, such as a failed
allocation or a panic while another panic unwinds; every call from R catches
an ordinary panic and returns it as an R error.

Review the archive and installed library against the
[CRAN policies](https://cran.r-project.org/web/packages/policies.html),
particularly source size, copyright coverage of linked dependencies, build
time, and test time.

## Learn more

- Package help: `?yamaa_spec`, `?yamaa_project`, `?yamaa_read_odm`,
  `?yamaa_lint`, `?yamaa_schema`, `?yamaa_conditions`, `?yamaa_info` and
  `?yamaa_selftest`.
- The [repository README](../../README.md#example) shows a complete
  specification, and the [benchmarks](https://elong0527.github.io/yamaa/benchmark/)
  provide many more.
- Language contracts: [project files](../../rules/profiles/project.md),
  [resource paths](../../rules/reference/files.md),
  [SAS sources](../../rules/reference/sas.md) and
  [submission documents](../../rules/README.md#rule-files).
- [Resource limits](../../rules/limits.md): the fixed limits and what a
  refusal reports.
