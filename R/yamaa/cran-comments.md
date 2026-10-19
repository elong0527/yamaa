## Submission

This is a new submission. yamaa builds clinical data specifications through
a shared Rust engine, which the package compiles at installation.

## Test environments

To be completed from the checks of the 1.0.0 candidate that is submitted:
`R CMD check --as-cran` on Linux, macOS and Windows with R 4.4, R-release and
R-devel, win-builder (R-release, R-devel and R-oldrel) and the macOS builder.
The package is also installed with the network disabled on Linux x86_64 and
aarch64, macOS and Windows.

## R CMD check results

To be completed from the same checks. These notes are expected, and why:

- `checking CRAN incoming feasibility`: a new submission, and the size of
  the tarball, about 14.5 MB. About 11 MB of it is `src/rust/vendor.tar.xz`,
  the Rust crates that the package compiles, which CRAN's guidance for
  packages that use Rust asks to be bundled rather than downloaded. The
  Repository Policy allows a modestly increased size limit to be requested
  for such bundled sources; we request one.
- `checking installed package size`: about 35 MB, of which about 26 MB is the
  shared object, which links the Rust engine statically.

## Rust

The package follows "Using Rust in CRAN packages":

- The crates.io crates that `src/rust/Cargo.lock` selects are bundled in
  `src/rust/vendor.tar.xz`, an xz-compressed tar written with
  `cargo vendor --locked --versioned-dirs` by `tools/vendor_crates.py`, which
  the package carries. Nothing is downloaded at installation.
- `SystemRequirements: Cargo (Rust's package manager), rustc (>= 1.90.0)`.
  1.90 is the minimum that the workspace declares (`rust-version`, edition
  2024); it is newer than the two-year-old Cargo that the guidance suggests
  testing with, and the build stops with a clear message on an older rustc.
- `src/Makevars` and `src/Makevars.win` look for `cargo` and `rustc` on the
  PATH and in `~/.cargo/bin`, ask the user to install them when they are
  missing, and print both versions before compiling. They run
  `cargo build --locked --offline -j 2` with a Cargo home below
  `src/rust/target`, which they remove after linking with the rest of the
  build tree, so the installation uses at most two jobs and writes nothing
  outside the build directory and the temporary directory. They run Cargo
  from `src/`, so a rustup installation never switches or installs a
  toolchain.
- `Authors@R` credits the authors of the bundled crates, and `inst/AUTHORS`
  lists each crate with the authors, license and repository it declares. The
  `Copyright` field names `inst/LICENSE-THIRD-PARTY.txt`, which reproduces
  the license, copyright and NOTICE texts of every bundled crate.
- The archive contains no binary executable code. `wit-bindgen`, a crate
  that only WebAssembly targets build, ships prebuilt WebAssembly objects;
  they are left out, and their C sources remain.

## Reverse dependencies

There are none: this is a new package.
