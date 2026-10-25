## Submission

This is a new submission.

## R CMD check results

The following notes may occur:

- `checking CRAN incoming feasibility ... NOTE`: new submission. SDTM and
  ADaM are standard abbreviations in clinical trials, expanded in DESCRIPTION.
- `checking installed package size ... NOTE`: the installation is
  about 22 MB, almost all the statically linked Rust shared library. Symbols
  and Rust standard-library debug information are retained. Test inputs stay
  in `tests/` and are not installed.

## Rust

Installation uses preinstalled Cargo and rustc (>= 1.82.0); it never installs
a toolchain or downloads crates. Locked crate sources are bundled, with no
toolchain file or prebuilt executable code. Compilation uses at most two
Cargo jobs and writes only to the build and temporary directories.
`Authors@R`, `inst/AUTHORS`, license notices and `inst/COMPONENTS.tsv`
credit the bundled components and their copyright holders.

## Reverse dependencies

None; this is a new package.
