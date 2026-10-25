## Submission

This is a new submission. yamaa builds clinical data specifications through
a shared Rust engine, which the package compiles at installation.

The source package carries a small base-R smoke suite. It runs the embedded
format selftest and checks the public specification/project API, R function
activation, progress callbacks and saved output against an authored study.
It needs no suggested packages. The full unit, regression and conformance
suites remain CI release gates against the installed package; their scripts
and large fixture trees are not shipped for CRAN to run.

## Test environments

`R CMD check --as-cran` on the package's source tarball, with the settings of
CRAN's incoming checks, two Cargo jobs and every network connection refused,
without the remote checks and the PDF and HTML manuals. Cargo and rustc are
1.82.0, the declared minimum, unless another version is named.

- Ubuntu 24.04 x86_64: R-devel (2026-10-22 r90748), R 4.6.1 and R 4.2.0, and
  R-devel without the suggested packages (`_R_CHECK_DEPENDS_ONLY_=true`).
- macOS 15 arm64: R-devel (2026-10-23 r90750), R 4.6.1 and R 4.2.0.
- Windows Server 2025 x64: R-devel (2026-10-22 r90748 ucrt), R 4.6.1 and
  R 4.2.0.
- R-hub's `gcc16` container, CRAN's r-devel-linux-x86_64-fedora-gcc
  configuration: R-devel (2026-10-21 r90732) built with GCC 16.2 on Fedora
  44, with Fedora's Cargo and rustc 1.99.0.
- R-hub's `clang-asan` container, the clang-ASAN configuration of CRAN's
  additional checks: R-devel (2026-10-21 r90732) built with clang 23.1 and
  libc++ on Ubuntu 26.04, which compiles the package's C, the bundled
  ReadStat and zstd sources included, with AddressSanitizer and
  UndefinedBehaviorSanitizer.

The package is also installed with the network disabled on Linux x86_64 and
aarch64, macOS and Windows. win-builder (R-devel, R-release and R-oldrel)
and the macOS builder run when the maintainer prepares the submission.

## R CMD check results

0 errors | 0 warnings | 1 note in each environment above with R-devel and
R 4.6.1; R 4.2.0 also notes the installed size. The notes, and why they
remain:

- `checking CRAN incoming feasibility ... NOTE`: a new submission, and the
  size of the tarball, about 11.0 MB. About 8.8 MB of it is
  `src/rust/vendor.tar.xz`, the Rust crates that the package compiles, which
  CRAN's guidance for packages that use Rust asks to be bundled rather than
  downloaded. The Repository Policy allows a modestly increased size limit to
  be requested for such bundled sources; we request a limit of 15 MB.
- `checking installed package size ... NOTE` (R 4.2.0; R 4.6.1 and R-devel
  report it as INFO): 25.2 MB on Linux, 20.4 MB on macOS and 17.7 MB on
  Windows, almost all of it the shared object (24.8, 20.0 and 17.3 MB), which
  links the Rust engine statically: most of it is machine code, and the
  symbols and the debug information of the Rust standard library are kept, as
  the policy asks. The inputs that the package's tests read are in `tests/`,
  which R does not install.

## Rust

The package follows "Using Rust in CRAN packages":

- The crates.io crates that `src/rust/Cargo.lock` selects are bundled in
  `src/rust/vendor.tar.xz`, an xz-compressed tar written with
  `cargo vendor --locked --versioned-dirs` by `tools/vendor_crates.py`, which
  the package carries. A crate that no supported platform compiles keeps
  only its manifest, which Cargo reads to resolve the lock. Nothing is
  downloaded at installation.
- `src/rust` holds only the crates that the package builds, with a
  `Cargo.lock` trimmed from the project's. The package has no
  `rust-toolchain.toml` or `rust-toolchain` file anywhere, the bundled crates
  included, so the build uses the Cargo and rustc that `configure` finds and
  never selects another toolchain.
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
  The Makevars pass that directory to Cargo, so a `CARGO_TARGET_DIR` set for
  other projects does not move the build; only `YAMAA_R_CARGO_TARGET_DIR`,
  which the project's CI sets so that its installations share one Cargo
  build, names another. Without it, the build also ignores a Cargo build
  directory and a rustc wrapper, such as a compiler cache, that the
  environment or a `.cargo/config.toml` in a parent directory sets.
- `Authors@R` credits the authors of the bundled crates and of the bundled
  ReadStat C sources as contributors and copyright holders. `inst/AUTHORS`
  lists every bundled third-party crate with the authors and repository that
  it declares and the license that yamaa relies on.
- The `Copyright` field names `inst/LICENSE-THIRD-PARTY.txt`, which
  reproduces the license, copyright and NOTICE texts of the third-party
  crates that the package links on any of its supported platforms, and the
  licensing of the Rust standard library, from the notice that the Rust
  project ships with its toolchain. The other bundled crates are not linked:
  the procedural macros and the crates that build scripts use, whose license
  files stay in `src/rust/vendor.tar.xz` as `cargo vendor` wrote them, and
  the crates that no supported platform compiles, bundled as their manifests
  alone.
- `inst/COMPONENTS.tsv` lists every bundled component, the ReadStat and zstd
  C sources included, with its version, license, source and purpose.
  `configure` appends one row to the copy that R installs, for the Rust
  standard library that rustc links into the package, with the version that
  rustc reports; `cleanup` removes it, so the archive carries the file
  unchanged.
- Where a crate or component is offered under a choice of licenses,
  `inst/AUTHORS`, `inst/LICENSE-THIRD-PARTY.txt` and `inst/COMPONENTS.tsv`
  name only the one that yamaa relies on, such as BSD-3-Clause for the zstd
  C library.
- The archive contains no binary executable code. `wit-bindgen`, a crate
  that only WebAssembly targets build, ships prebuilt WebAssembly objects;
  like every crate that no supported platform compiles, it is bundled as
  its manifest alone.

## Reverse dependencies

There are none: this is a new package.
