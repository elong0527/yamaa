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

0 errors | 0 warnings | 1 note with R 4.6.1 and R-devel; R 4.4 also notes
the installed size. The notes, and why they remain:

- `checking CRAN incoming feasibility ... NOTE`: a new submission, and the
  size of the tarball, about 14.5 MB. About 11 MB of it is
  `src/rust/vendor.tar.xz`, the Rust crates that the package compiles, which
  CRAN's guidance for packages that use Rust asks to be bundled rather than
  downloaded. The Repository Policy allows a modestly increased size limit to
  be requested for such bundled sources; we request one.
- `checking installed package size ... NOTE` (R 4.4; R 4.6.1 and R-devel
  report it as INFO): about 35 MB on Linux. The shared object, about 26.5 MB,
  links the Rust engine statically: about 16 MB is machine code, and the
  symbols and the debug information of the Rust standard library are kept, as
  the policy asks. The inputs that the package's tests read are about 7.5 MB,
  in `inst/specification-original`, `inst/grammar-original` and
  `inst/host-fixtures`.

## Rust

The package follows "Using Rust in CRAN packages":

- The crates.io crates that `src/rust/Cargo.lock` selects are bundled in
  `src/rust/vendor.tar.xz`, an xz-compressed tar written with
  `cargo vendor --locked --versioned-dirs` by `tools/vendor_crates.py`, which
  the package carries. Nothing is downloaded at installation.
- `SystemRequirements: Cargo (Rust's package manager) (>= 1.82.0), rustc
  (>= 1.82.0)`. 1.82 is the `rust-version` that the bundled workspace
  declares (edition 2021). Rust 1.82.0 was released on 2024-10-17, so it is
  at least the two-year-old Cargo that the guidance suggests testing with,
  and CI builds and checks the package with it.
- `configure` and `configure.win` run `tools/configure.R`, which looks for
  `cargo` and `rustc` on the PATH and then in `~/.cargo/bin`, requires each to
  be at least the declared `rust-version`, prints both versions in the
  installation log before compilation starts, and otherwise stops and asks
  the user to install them with the system's package manager or rustup. It
  never installs Rust, and sets `RUSTUP_AUTO_INSTALL=0` so that rustup does
  not either.
- `src/Makevars` and `src/Makevars.win` run
  `cargo build --locked --offline -j 2` with `MAKEFLAGS` cleared, because
  Cargo otherwise joins the jobserver of a parallel make and runs more jobs.
  The Cargo home is below `src/rust/target`, which is removed after linking
  with the rest of the build tree, so the installation uses at most two jobs
  and writes nothing outside the build directory and the temporary directory.
- `Authors@R` credits the authors of the bundled crates and of the bundled
  ReadStat C sources as contributors and copyright holders. `inst/AUTHORS`
  lists each crate with the authors, license and repository that it declares,
  and the `Copyright` field names `inst/LICENSE-THIRD-PARTY.txt`, which
  reproduces the license, copyright and NOTICE texts of every bundled crate.
- The archive contains no binary executable code. `wit-bindgen`, a crate
  that only WebAssembly targets build, ships prebuilt WebAssembly objects;
  they are left out, and their C sources remain.

## Reverse dependencies

There are none: this is a new package.
