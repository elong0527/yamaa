## Submission

This is a new submission. yamaa builds clinical data specifications through
a shared Rust engine, which the package compiles at installation.

The source package carries a small base-R smoke suite. It runs the embedded
format selftest and checks the public specification/project API, R function
activation, progress callbacks and saved output against an authored study.
It needs no suggested packages. The full unit, regression and conformance
suites remain CI release gates against the installed package; their scripts
and large fixture trees are not shipped for CRAN to run.

The short introductory vignette links the full documentation and presents
one self-contained example using temporary files. Its example is not
evaluated during rendering; it is checked separately against the installed
package. `R CMD build` builds and includes its HTML.

## Test environments

These comments are a submission draft. Final release qualification and the
maintainer's win-builder and macOS builder results are pending; replace this
status with their results before submission.

The source package with the minimal vignette and vendored development-file
exclusions has passed `R CMD check --as-cran --no-manual` on Ubuntu 24.04
x86_64 with R 4.6.1 and Cargo/rustc 1.99.1. The local dependency-only check
used CRAN's incoming-check settings, two Cargo jobs and no network, with
remote checks and the PDF/HTML manuals disabled. The check rebuilt the
vignette with knitr, rmarkdown and Pandoc available; arrow was unavailable.
The full suggested-package R checks and all regression suites also passed
pull-request CI on Ubuntu with R 4.6.1.

The builder-free dependency-only check sets `_R_CHECK_DEPENDS_ONLY_=true`
and `_R_CHECK_FORCE_SUGGESTS_=false` and passes `--ignore-vignettes` while
retaining the base-R smoke tests. This local check also passed with all four
suggested packages unavailable. Normal checks validate the vignette with its
builders installed.

The final archive still needs the configured Linux, macOS and Windows
checks with R-devel, R 4.6.1 and R 4.2.0 and the declared minimum Rust 1.82.0,
the Linux noSuggests and manual checks, and R-hub's gcc16 and clang-asan
checks. Earlier platform results predate the vignette and development-file
exclusions and do not qualify this archive. The maintainer's external
builders must check the unchanged final release tarball.

## Size and timing

The measured source archive is 9,412,481 bytes (9.41 MB), including
7,514,532 bytes (7.51 MB) in `src/rust/vendor.tar.xz`, below the Repository
Policy's 10 MB guideline. The installed package on the local Ubuntu/R 4.6.1
check is 21.9 MB, including 21.5 MB in `libs`; platform/toolchain-specific
sizes must be refreshed from final qualification.

The most recent Windows checks before the vignette and development-file
exclusions completed in 491 seconds with R-devel, 517 seconds with R 4.6.1,
and 493 seconds with R 4.2.0 (8.2-8.6 minutes). These are the whole CRAN-check
step times, including installation, rather than the workflow-job times that
also include provisioning. All three were below the 10-minute target.
Confirm these timings on the final archive and external builders.

## R CMD check results

The completed Ubuntu/R 4.6.1 checks reported `Status: OK` (0 errors,
0 warnings, 0 notes). With remote checks disabled, CRAN's new-submission
note is not produced. The local installed size and unavailable arrow were
reported as INFO. Final platform results remain pending. The checker allows
only the following notes, whose reasons are unchanged:

- `checking CRAN incoming feasibility ... NOTE`: this is a new submission.
  R versions may also note the tarball size. The current archive is within
  the 10 MB guideline; most of it is the bundled Rust sources required for
  offline installation. Under the Repository Policy's provision for a
  modestly increased bundled-source allowance, we request a limit of 15 MB.
- `checking installed package size ... NOTE` (R 4.2.0; R 4.6.1 and R-devel
  report it as INFO): almost all of the installed size is the shared object,
  which links the Rust engine statically. Most of it is machine code; the
  symbols and Rust standard-library debug information are retained as the
  policy asks. Test inputs stay in `tests/`, which R does not install.

## Rust

The package follows "Using Rust in CRAN packages":

- The crates.io crates that `src/rust/Cargo.lock` selects are bundled in
  `src/rust/vendor.tar.xz`, an xz-compressed tar written with
  `cargo vendor --locked --versioned-dirs` by `tools/vendor_crates.py`, which
  the package carries. A crate that no supported platform compiles keeps
  only its manifest, which Cargo reads to resolve the lock. Nothing is
  downloaded at installation.
- `tools/vendor_exclusions.toml` names the audited development directories
  left out of specific crate versions: tests, benchmarks, standalone examples
  and CI files. Production/build inputs, embedded documentation and license
  files remain, and unaudited compiled crate versions keep their development files.
  The omitted files are listed in `inst/AUTHORS`; their entries alone are
  removed from Cargo's file-checksum records, preserving the package checksums.
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
