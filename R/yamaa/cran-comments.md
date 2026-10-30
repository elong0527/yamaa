## Update from 0.6.2

This update adds in-memory data-frame inputs, codelist decoding, project
inheritance and additional Define-XML support, and fixes input handling and
host error reporting. The Windows build fixes from 0.6.2 are retained.

## Submission timing

The 0.7.0 release is published on GitHub and PyPI only, and is not
submitted to CRAN. Before a later release is submitted, the maintainer
checks its unchanged release source archive with win-builder (R-devel) and
the macOS builder, and records their results with the release qualification.

## Test environments

- GitHub Actions: R 4.2.0, R-release and R-devel on Ubuntu 24.04, macOS 15
  (arm64) and Windows Server 2025.
- R-hub: the gcc16, gcc-asan, clang-asan and valgrind containers.

## R CMD check results

The release checks require 0 errors and 0 warnings, allowing only the two
notes explained below. The maintainer records the final results with the
release qualification before submission.

- `checking CRAN incoming feasibility ... NOTE`: SDTM and ADaM are standard
  abbreviations in clinical trials, expanded in DESCRIPTION. A check against
  an index that has not yet recorded 0.6.2 may also report a new submission.
  The tarball is about 9.4 MB, most of it the vendored Rust crates that the
  build compiles offline.
- `checking installed package size ... NOTE`: the installation is about
  14 MB on Windows, 17 MB on macOS and 22 MB on Linux, almost all the
  statically linked Rust shared library. Symbols and Rust standard-library
  debug information are retained. Test inputs stay in `tests/` and are not
  installed.

## Rust

Installation uses preinstalled Cargo and rustc (>= 1.82.0); it never installs
a toolchain or downloads crates. Locked crate sources are bundled, with no
toolchain file or prebuilt executable code. Compilation uses at most two
Cargo jobs and writes only to the build and temporary directories.
`Authors@R`, `inst/AUTHORS`, license notices and `inst/COMPONENTS.tsv`
credit the bundled components and their copyright holders.

## Reverse dependencies

The maintainer rechecks reverse dependencies before submission.
