## Resubmission

This is a new submission, resubmitted. In the incoming pretest of 0.6.0, the
package failed to install on Windows: `src/Makevars.win` gave Cargo R's `CC`
as the linker, which ends in a space there, so Cargo found no linker `gcc `
for the build scripts. In 0.6.1:

- `src/Makevars.win` gives Cargo the first word of `CC` as the linker.
- Our Windows checks now build with a Rust toolchain for the GNU host,
  `x86_64-pc-windows-gnu`, as CRAN's Windows machines do.

## Test environments

- GitHub Actions: R 4.2.0, R-release and R-devel on Ubuntu 24.04, macOS 15
  (arm64) and Windows Server 2025.
- R-hub: the gcc16, gcc-asan, clang-asan and valgrind containers.

## R CMD check results

0 errors | 0 warnings | up to 2 notes

- `checking CRAN incoming feasibility ... NOTE`: new submission. SDTM and
  ADaM are standard abbreviations in clinical trials, expanded in DESCRIPTION.
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

None; this is a new package.
