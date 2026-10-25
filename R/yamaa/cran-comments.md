## Submission

This is a new submission.

## Test environments

- Ubuntu 24.04 x86_64: R 4.6.1, Cargo and rustc 1.99.1.

Final release qualification and win-builder/macOS builder results are pending
and will be recorded before submission.

## R CMD check results

The Ubuntu/R 4.6.1 check reports 0 errors and 0 warnings. The following notes
may occur:

- `checking CRAN incoming feasibility ... NOTE`: new submission. SDTM and
  ADaM are standard model abbreviations, expanded in DESCRIPTION. Preliminary
  source measurements are about 9.4 MB, including 7.5 MB of bundled Rust
  crates needed for offline installation. Under the Repository Policy's
  bundled-source provision, we request a limit of 15 MB.
- `checking installed package size ... NOTE`: the Ubuntu installation is
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
