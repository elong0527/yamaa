# Release and migration

This page records what a yamaa release supports, the behavior decisions it
keeps, and how to migrate older specifications. Release status, evidence and
open gates live in GitHub issues and pull requests, not here.

## Supported platforms

Each row is a platform, and each column a package form that a release
publishes. A cell names the CPython or R series with which native
qualification installs and tests that form on that platform, with the oldest
glibc that a Linux wheel needs, or the Rust target that the command-line
binary is built for. A floor, as in R 4.2 or later, is qualified with the floor
and R-release, and a cell that says "no" means that a release has no such
package for that platform. `tools.validation.check_supported_platforms`, which
`make check-repository` runs, fails when the table and the native matrix that
`tools/ci/native.py` declares disagree.

| Platform | Python wheel | Python source archive | R source package | R binary package | Command-line binary |
| --- | --- | --- | --- | --- | --- |
| Linux x86_64 | CPython 3.12 to 3.15 on glibc 2.28 or later | CPython 3.12 to 3.15 | R 4.2 or later | no | `x86_64-unknown-linux-musl` |
| Linux aarch64 | CPython 3.12 to 3.15 on glibc 2.28 or later | CPython 3.12 to 3.15 | R 4.6 | no | `aarch64-unknown-linux-musl` |
| macOS arm64 | CPython 3.12 to 3.15 | CPython 3.12 to 3.15 | R 4.2 or later | R 4.5 and 4.6 | `aarch64-apple-darwin` |
| Windows x64 | CPython 3.12 to 3.15 | CPython 3.12 to 3.15 | R 4.2 or later | R 4.5 and 4.6 | `x86_64-pc-windows-msvc` |

- The Linux wheels are `manylinux_2_28`, so they need glibc 2.28 or later,
  for example RHEL 8, Ubuntu 20.04 or Debian 10; older systems build from the
  Python source archive. Each Linux wheel is built in the manylinux_2_28
  container of its architecture, and native qualification also installs it
  on AlmaLinux 8, which has glibc 2.28, and runs the conformance corpus there
  with that system's Python 3.12.
- The R source package needs R 4.2.0 or later, qualified on the oldest
  admitted patch with the minimum Rust (#3561). On Linux x86_64, macOS arm64 and
  Windows x64 it passes `R CMD check --as-cran` with only the notes that
  `R/yamaa/cran-comments.md` explains: with R 4.6.1, and in CRAN's incoming
  checks with R-devel, R-release and R 4.2.0 and the minimum Rust. On Linux
  x86_64 it also passes with its C compiled under AddressSanitizer and
  UndefinedBehaviorSanitizer, with R-devel in R-hub's `clang-asan`
  container. The separate R-CMD-check workflow also checks the latest R 4.2
  patch on Linux.
- On Linux aarch64, native qualification uses R-release alone. Its
  `native-r` job installs the R source package that a release publishes,
  runs the conformance corpus and the installed R suites with it, and passes
  `R CMD check --as-cran` with R 4.6.1, as on the other platforms. With the
  network disabled, it also builds and installs that package and runs the
  conformance corpus with it: its `native-offline-source` job, with the
  pinned Rust, and its `native-offline-toolchain` job, with the minimum Rust
  installed from its standalone installer. CRAN's incoming checks, the
  `native-cran` jobs, do not run on aarch64, and no lane there uses R 4.2.
- The R binary packages are for R-release and R-oldrel, as CRAN builds them
  (#3030 D48). Each is built for one R series and works only with R of that
  series, and the macOS binaries need macOS 11 or later, the first macOS for
  Apple silicon; `install.packages()` installs each with no Rust, Rtools or
  network. With any other R series, R 4.2 among them, and on Linux, which
  gets no binary packages (D54), users install from source: the R source
  package of the GitHub release, which builds with its vendored crates and a
  Rust toolchain; CRAN, from 0.6.0; or, on a Linux machine without
  Rust 1.82 or later, the offline toolchain route, which carries Rust in from
  its standalone installers without rustup or a network
  ([Installing in a restricted environment](articles/restricted-environments.md#the-offline-rust-toolchain-route)).
- The `yamaa` command line ships as standalone binaries, which need no
  Python, R or Rust (#3030 D12, D49). The Linux binaries link the musl C
  library statically, so they run on any Linux of their architecture; the
  Windows binary links the C runtime statically; and the macOS binary needs
  macOS 11 or later. Each is a GitHub release asset,
  `yamaa-<version>-<target>.zip` for its target above, with no installer,
  and ships unsigned, as [command-line binaries](#command-line-binaries)
  describes.
- Other runtimes and architectures, macOS x86_64 among them (#3030 D49;
  CRAN may still build the R package there), free-threaded Python, and the
  Python and R packages on musl systems such as Alpine Linux, are not
  supported. R is distributed as a GitHub source tarball and binary
  packages, and from 0.6.0 on CRAN, as [CRAN submission](#cran-submission)
  describes.

Source builds compile the Rust workspace, so they need Rust 1.82.0 or later
with Cargo and a C compiler. Both source packages carry the locked crates and
build offline, as [Source package contents](#source-package-contents)
describes. pip's build isolation fetches `maturin` 1.9.6 for the Python source
archive, so an offline Python build needs `maturin` installed, for
`pip install --no-build-isolation --no-index`. Windows R builds also need
the Rtools version for their R series (Rtools42 for R 4.2) and the GNU Rust
target, as
[Supported dependency combinations](#supported-dependency-combinations)
describes. [Installing in a restricted environment](articles/restricted-environments.md)
gives every route for a machine without internet access, each with the native
jobs that qualify it.

`.github/workflows/native.yml` builds, installs and tests every supported
combination on manual dispatch and weekly on main. A release is cut from a
commit whose complete native run passed.

The native run builds and installs both source archives that a release
publishes with the network disabled, on Linux x86_64 and aarch64, macOS
arm64 and Windows x64: its `native-offline-source` jobs. pip installs the
Python source archive with `--no-build-isolation --no-index` into a Python
that already has `maturin`, and `R CMD INSTALL` installs the R source
package. On Linux both run in a container without a network: the Python
archive in the manylinux_2_28 image, whose glibc 2.28 is RHEL 8's, and the R
package in the `rocker/r-ver` image of R 4.6.1. On macOS and Windows they run
behind a proxy that refuses every connection. Each installed package then
runs its selftest, `yamaa.selftest()` or `yamaa_selftest()`, without a
network: the job's log keeps the report as installation qualification
evidence, and the job fails unless the selftest reports `succeeded`. The
package then runs the conformance corpus, also without a network.

The native run also installs each wheel that a release publishes as a
restricted machine installs
[files carried in](articles/restricted-environments.md#from-files-carried-in):
its `native-offline-wheel` jobs, on every platform with CPython 3.12 and
3.15. The wheels are abi3, so the oldest and the newest supported CPython
bound the supported interpreters that each wheel serves. While the network
is up, pip downloads the wheel's runtime dependencies, at the versions that
`python/uv.lock` pins and as wheels alone, into one directory with the
wheel. Then, with the network disabled, pip installs the wheel from that
directory alone, with `--no-index --find-links`, into a new virtual
environment, which builds nothing. The installed package must import with
the wheel's version, and it then runs its selftest: the job's log keeps the
report, and the job fails unless it reports `succeeded`. On Linux the
installation and the selftest run in containers without a network, from the
manylinux_2_28 image with its CPython of the lane's version, and on macOS
and Windows behind the same refusing proxy.

The native run also proves the offline toolchain route (#3030 D54) on Linux
x86_64 and aarch64: its `native-offline-toolchain` jobs. While the network is
up, each downloads the standalone installer archive of the minimum Rust for
the runner's target from static.rust-lang.org, such as
`rust-1.82.0-x86_64-unknown-linux-gnu.tar.xz`, and GnuPG checks the signature
beside it, its `.asc` file, against the committed public key at
`tools/ci/keys/rust-key.gpg.ascii`, in a fresh keyring with automatic key
retrieval disabled. The installer download cannot replace the trusted key.
That file is the ASCII-armored key published at
`https://static.rust-lang.org/rust-key.gpg.ascii`. A maintainer refreshes it
only in a reviewed change, after checking the Rust project's signing-key
announcement through an independent official source; a failed signature
never triggers an automatic key refresh. Then, in containers without a
network, the archive's own `install.sh` installs rustc, Cargo and the standard
library into a prefix, with no rustup, and both source archives build and
install with that toolchain alone, in the images and with the commands of the
`native-offline-source` jobs. Each installed package then passes its selftest
and runs the conformance corpus without a network.

The native run builds the R binary packages that a release publishes from
the R source package that it publishes, with `R CMD INSTALL --build` and
each covered R series: its `native-r-binary` jobs, behind the same refusing
proxy, on Windows with Rtools45 and the GNU Rust target, and on macOS with
`MACOSX_DEPLOYMENT_TARGET` set to 11.0, which each job compares with the
minimum macOS that the binary's `yamaa.so` declares. Its
`native-r-binary-install` jobs then install each binary as a user does, with
`install.packages(path, repos = NULL)`, on a runner of its platform from
which the image's Rust has been uninstalled and that has no Rtools; the log
shows that neither the shell nor R finds a Rust tool or an Rtools directory.
The installation, the installed package's `yamaa_selftest()`, the
conformance corpus and the installed R suites run behind the refusing proxy.
The job's log keeps the selftest's report, and the job fails unless it
reports `succeeded`. The cohort comparison reads each binary's reports
beside those of its platform's wheel: on Windows the wheel is built with
MSVC and the binary with Rtools' GNU toolchain.

The native run builds the command-line binaries that a release publishes,
each for its Rust target on a runner of its platform: its `native-cli` jobs.
Each builds the `yamaa` executable of `rust/crates/yamaa-cli` with the pinned
toolchain and the `dist` profile, release without the symbol table, as the
wheels are built. The Linux targets link the musl C library that Rust
bundles, the runner's `musl-gcc` compiles the bundled ReadStat and zstd C
sources, and the job requires `file` to call the binary static and `readelf`
to find no program interpreter and no shared library. On Windows
`-C target-feature=+crt-static` links the C runtime statically, and
`dumpbin` must list no C runtime DLL. On macOS `MACOSX_DEPLOYMENT_TARGET` is
11.0, which the job compares with the minimum macOS that the binary
declares, and every library that the binary loads must be the system's.
Each job archives its binary with the command line's notices and checks the
archive with `tools/packaging/cli_archive.py` and
`tools.packaging.check_package_notices`. Its `native-cli-install` jobs then
unpack and run each archive on a fresh runner of its platform with the
network disabled. While the network is up, each job checks out the
repository and sets up Python, but no Rust, and
`tools.commands.cli_conformance` prepares the conformance corpus: it copies
every benchmark, which the binary reads as 1.x files (#3030 D122), and plans
the copies, leaving out those whose project supplies project functions,
which the binary cannot call.
Then the job unpacks the archive and runs `yamaa --version`, whose first
line must name the archive's version, and `yamaa selftest`, which must
succeed; the log keeps both reports. It then runs the corpus through the
unpacked binary, and the comparator judges each run against the expected
files: the job fails on any difference. The unpacking and every run of the
binary have no network: on Linux they run in containers without a network,
from the manylinux_2_28 image, whose Python runs the conformance runner, and
on macOS and Windows behind the refusing proxy. The cohort comparison reads
each archive's saved outputs beside its platform's wheel with the build
Python and the R reports beside that wheel: for each case of the
complete-report cohort, the command line's outcome and saved files must
equal both installed hosts' reports, CSV byte for byte.

On Windows the native run also runs the conformance corpus on a network share
(#3030 D15): its `native-share` jobs. The runner shares a directory of its own
over SMB as `\\localhost\yamaa-share`, and the wheel that a release publishes
for Windows runs the corpus with every specification, project, input and
output on that share, through its UNC path in one job and through a drive
mapped to the share in the other. The conformance runner stages each case in
the shared directory and names it in every call through the share or the
drive alone, so the engine resolves the share itself, and the corpus's
symbolic link, which the file server reports as a reparse point, is refused
there as on a local drive (FILE-8). No job runs against a Samba server, which
by default resolves a link itself and serves its target in its place: FILE-8
and [Inputs, outputs and paths](articles/api.md#inputs-outputs-and-paths)
state that limit.

The native run also runs CRAN's incoming checks on the R source archive that a
release publishes, the bytes of its `native-r-ubuntu-24.04` artifact: its
`native-cran` jobs. Each runs `R CMD check --as-cran` through
`tools/commands/check_r.py`, which fails on any NOTE, WARNING or ERROR that
`R/yamaa/cran-comments.md` does not explain and on a tarball above the 15 MB
that it requests. The build reads the vendored crates behind a proxy that
refuses connections and runs two Cargo jobs.

- With R-devel, R-release and R 4.2.0, on Linux, macOS and Windows, Cargo and
  rustc are the workspace's `rust-version`, 1.82: the two-year-old toolchain
  that CRAN's guidance asks a package to be tested with.
- With R-devel on Linux and none of the package's suggested packages
  installed, as CRAN's noSuggests check runs: `_R_CHECK_DEPENDS_ONLY_=true`
  gives the tests only the Depends and Imports, and with
  `_R_CHECK_FORCE_SUGGESTS_=false` R reports the missing packages instead of
  stopping. This lane passes `check_r.py --ignore-vignettes`, because its
  missing Suggests include the vignette builders. Every other CRAN lane
  validates and rebuilds the vignette with its builders and Pandoc installed.
  The tests skip, with a message, what needs `arrow` or `jsonlite`.
- With R-devel on Linux and the PDF and HTML manuals, which CRAN's incoming
  checks build and validate and the other jobs leave out with `--no-manual`:
  the job installs pdflatex with the LaTeX packages and fonts of R's manual,
  and HTML Tidy, and `check_r.py --manual` fails unless R reports both manual
  checks OK.
- In R-hub v2's `gcc16` container, CRAN's r-devel-linux-x86_64-fedora-gcc
  configuration (R-devel built with GCC 16 on Fedora 44, with CRAN's check
  settings), the build uses the Cargo and rustc that Fedora packages, as
  CRAN's Linux check servers use their distribution's.
- In R-hub v2's `clang-asan` container, the clang-ASAN configuration of
  CRAN's additional checks (R-devel built with clang 23 and libc++ on Ubuntu
  26.04, whose C compiler instruments packages with AddressSanitizer and
  UndefinedBehaviorSanitizer), the build uses the minimum Rust, installed
  from its standalone installer after GnuPG checks its signature, as in the
  `native-offline-toolchain` jobs. The Makevars give Cargo R's C compiler, so
  the bundled ReadStat and zstd C sources are instrumented as the package's
  own C is, which the job confirms in the installed shared object; the Rust
  code is not instrumented. `check_r.py` also fails on any sanitizer report
  in the installation, example or test output, since
  UndefinedBehaviorSanitizer reports and carries on.

Development PRs do not run the release lanes, so before a release also
dispatch R-CMD-check, Dependency graphs and Release compatibility on the
candidate. Release compatibility checks the minimum toolchain, the R source
package built with it, minimal format builds, Python 3.15 and minimum Python
saved artifact readers; dispatch CI for its minimum R saved artifact reader
checks.

A release has one identity, written once in `tools/packaging/release.toml`:
the final version X.Y.Z and, for a release candidate, its number N. Each
package format spells it in its own syntax, and
`tools.packaging.check_shared_release_versions` requires every package
manifest, lock and benchmark R environment to carry its spelling, and every
link of `python/README.md` that names a release tag to name its tag:

| Release | Python | Cargo | R | Tag |
| --- | --- | --- | --- | --- |
| Final release | `X.Y.Z` | `X.Y.Z` | `X.Y.Z` | `yamaa-vX.Y.Z` |
| Release candidate N | `X.Y.ZrcN` | `X.Y.Z-rc.N` | `X.Y.Z` | `yamaa-vX.Y.Z-rc.N` |

An R version cannot mark a candidate, so the R package keeps the final version
through the candidates (#3030 D53), and R does not offer the final release as
an update to a candidate.

The commit that changes the identity also re-pins the tag links of
`python/README.md`, the wheels' PyPI description: the check fails until they
name the new tag, and they return 404 on GitHub until that tag is pushed.

## Stability contract

From yamaa 1.0.0, every 1.x release keeps the 1.0 interface. A project, a
program or a script that works with 1.0.0 works the same way with every later
1.x release, with the same results and the same saved bytes. Only a 2.0
release may break the interface, and its release notes say what breaks
(#3030 D20). Release candidates are not part of the contract: the freeze
starts at 1.0.0 (#3030 D1).

**What is frozen.** Each surface is named by the files that hold it.

- Files: the four file types, `project`, `spec`, `codelists` and `function`,
  as the committed JSON Schemas in `yaml/json/` give them: every field's
  name, kind, type, default and meaning, and the verbs and verifications with
  their parameters. Also the closed grammars in `yaml/grammar/`, and the
  closed vocabularies, such as column types, profiles and the CDISC lists in
  `yaml/vocabulary/cdisc.yaml`. Each release's JSON Schema for a file type is
  that committed file at the release's tag; for `spec` in 1.0.0 it is
  `https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v1.0.0/yaml/json/spec.schema.json`.
  From 1.0.0 the files keep their path, and they carry no `$id`
  (#3030 D161). The grammars at the tag, with the evaluation set of the
  expression language, `yaml/conformance/expressions.yaml`, also ship as one
  release asset, `yamaa-<Cargo version>-conformance.zip`, as
  [Publication](#publication) describes.
- Diagnostics: the condition names in `yaml/conditions.yaml` and the phases
  each may report; the condition and the rule that an existing failure
  reports; a `context` key that a benchmark's expected `error.yaml` pins,
  with its value (#3030 D156); and the twelve columns of `issues` and the
  fourteen of the run report, in order. Each release's condition catalogue,
  which `conditions()` returns, is also that release's committed
  `yaml/conditions.yaml` at its tag, as each JSON Schema is: every condition
  with its phases, message template and former name, and every condition a
  release retired; for 1.0.0 it is
  `https://raw.githubusercontent.com/elong0527/yamaa/yamaa-v1.0.0/yaml/conditions.yaml`.
  The rules that `conditions()` adds are those that the core registry's
  causes cite (#3700).
- Host functions and the command line: the frozen sections of
  `rust/interface/bindings.yaml`, as `rust/interface/manifest.txt` lists
  them. They hold the Python, R and command-line names; the arguments, with
  their names, order and defaults; result members; the values of `status`;
  return types; each host's type mapping; and the command line's commands,
  flags, exit statuses and `--format json` keys.
- The Rust facade: the public items of the `yamaa` crate, as
  `rust/crates/yamaa/public-api.txt` lists them. The internal crates
  `yamaa-core`, `yamaa-adapters`, `yamaa-application` and `yamaa-boundary`
  are not covered, nor
  are the native modules (`yamaa._native` and the R package's registered
  routines), which are internal (#3679).
- Saved artifacts: the bytes each profile writes (CSV, Parquet, SAS Transport
  version 5, Dataset-JSON 1.1, Define-XML 2.1 and the run report) for an
  admitted project and its inputs, on every supported platform. The Parquet
  profile asks only that a file read back the same
  ([PARQUET-4](../rules/reference/parquet.md#parquet-4)); yamaa's releases
  keep its bytes too, including the writer that its footer names in
  `created_by`, which is `parquet-rs version 55.2.0` in every 1.x release
  (#3030 D160).
- Printed text: the labels, order and separators of every summary line (a
  run's, a project run's, a submission's and a check's; a selftest's, with
  its `first failure:` line, its `not covered:` line, which only a build that
  leaves out a feature prints, and its information line; and that of a spec
  or project that has read nothing), the layout of the `first issue:` line
  and of a project run's member lines, and the `yamaa.Table:` header.

**What a minor release may add.** A minor release may add optional fields
whose absence keeps the 1.0 meaning; file types, verbs, verifications and
grammar functions; enumeration values; conditions, for new features only;
`context` keys, since a benchmark's expected context need only be contained
in the issue's (#3030 D156); host functions, members, and optional arguments
after the existing ones; commands, flags and JSON keys; and table columns
after the frozen ones. A file that uses a feature added in 1.N declares
`yamaa: "1.N"`, and an older engine refuses it as
`unsupported_language_version`, naming the version that it needs
([SCHEMA-2](../rules/reference/schema-language.md#schema-2)). A minor
release may also raise a platform floor, such as the minimum Python, R or
Rust, and its release notes announce it; a patch release never does.

**What a patch release may change.** Defect fixes, documentation, packaging
and platform support, and message text. A change that a repository such as
CRAN asks for after a release moves every host to the same patch version
(#3030 D51).

**What no 1.x release may do.** Rename, remove or retype a field, argument,
member, command, flag, column, condition, phase or enumeration value; drop a
pinned `context` key or change its value; add a required field or argument;
change a default; report a different condition or rule for an existing
failure; change the results or bytes of an admitted file; or refuse a file
that an earlier 1.x release admitted.

**Messages are not frozen.** Every `message` column, error text, the message
in a printed summary and the cells of a table preview may change in any
release. Programs match `condition` and `rule`, never `message`, and read
`context` as JSON, by key, since a release may add keys.

**Defect fixes.** Behavior that contradicts a rule is a defect, and fixing it
is not a break, even when bytes change. Each fix names the rule it restores,
adds a benchmark that pins the corrected bytes, and is listed in the release
notes. When a fix must change a frozen element, or a file of a benchmark that
the previous release had, it adds an entry for each to the interface check's
one exception file, `tools/validation/interface_exceptions.yaml` (#3442),
naming the rule it restores and the issue that records the owner's decision;
nothing else is exempt. When a rule itself is wrong, changing its statement
changes the language: the change waits for 2.0, or arrives in 1.x as a new
optional field.

**Enforcement.** The interface check,
`tools.validation.check_interface_stability`, runs in
`make check-repository`, and so in CI's repository job. It compares the
frozen surfaces with the newest final release of the same major, the tag
`yamaa-vM.m.p` with no candidate suffix, and refuses a break in the
conditions and their phases in `yaml/conditions.yaml`, the lines of
`rust/interface/manifest.txt` and those of
`rust/crates/yamaa/public-api.txt` (#3442), the named types and fields of
the four file types' JSON Schemas, and the functions, reducers, keywords,
operators, productions and reserved words of the grammars in
`yaml/grammar/` (#3444). It also refuses a (phase,
condition, rule) triple removed from the core diagnostic registry, a
removed rule card ID, and any change to the specification, input or
expected files of a benchmark present at that tag, an added file included;
its README and `benchmark.yaml` may change (#3445).

It enforces nothing while the major is 0. For the first release of a new
major, its candidates included, it lists every break against the newest final
release of the previous major, for that release's notes, and passes. Since
those benchmarks keep their expected files and CI compares each result with
them, a 1.x release reproduces 1.0's results and saved bytes. CI compares a
Parquet file by what it reads back as, so a control keeps its bytes:
`rust/crates/yamaa-adapters/tests/table_parquet_bytes.rs` saves fixed tables
and compares every byte with the files that the 1.0 writer saved, and
requires `created_by` to be `parquet-rs version 55.2.0` (#3475). The native
writer that replaced parquet 55.2.0 passes it with those files unchanged, and
any change to the writer must too (#3030 D160, #3878).

## Saved artifact readers

The minimum supported independent readers are:

| Reader | Minimum | Saved formats qualified |
| --- | --- | --- |
| Python Polars | 1.35.2 (2.0.0 on CPython 3.15) | CSV text and Parquet |
| Python PyArrow | 23.0.1 (26.0.0 on CPython 3.15) | CSV text and Parquet |
| R arrow | 23.0.1 | CSV text and Parquet |
| Python standard library | Supported CPython above | Dataset-JSON 1.1 |
| R jsonlite | 2.0.0 | Dataset-JSON 1.1, with `bigint_as_char = TRUE` |
| Python lxml | 6.1.3 | Define-XML 2.1 |

The Python package's `test` extra and R package `Suggests` declare these
floors.
The R readers are optional: public Build and Save use the shared native codecs.
CSV carries text rather than an embedded schema; controls supply an explicit
text schema and check exact authored CSV bytes. Its empty-field ingestion
semantics are described in the [CSV profile](../rules/reference/csv.md).
Parquet checks every logical type, nullable field, zone-free microsecond
timestamp, field and row order, full signed-i64 values, exact finite double
bits and empty-table schema. Read R integer columns through Arrow arrays, or
cast to text inside Arrow: converting to `bit64` loses `INT64_MIN` to its
missing-value sentinel. R doubles cannot represent every signed-i64 integer.

Dataset-JSON controls use column metadata to retain integer, float and temporal
meaning. Python's integer decoder must preserve the JSON token `-0` when it
belongs to a float column. jsonlite decodes that token as integer zero; R
qualification checks its mathematical value and the original signed token,
and pins every other finite float's bits. CSV ingestion maps collected empty
text to missing; Parquet retains it as present empty text. Both forms are
covered before Dataset-JSON publication. Define-XML has independent schema and
fixed-byte controls in the installed public Define suites.

The standalone `saved_readers` suites use shared independently authored
fixtures under `rust/tests/host-fixtures/`, build through installed Python and
R public APIs, remove the original inputs and specifications, then read the
held Save bytes. The Python readers decode the same captured byte buffer;
R arrow reads its installed host's corresponding captured buffer. Repeated
saves of one run must retain identical bytes. Ordinary CI tests the minimum
readers; every native wheel/source and R source lane also runs these controls
with the minimum versions for its interpreter. SAS datasets and Transport are
input formats; the published dataset profiles remain CSV and Parquet. SAS
Transport version 5 and Dataset-JSON files and Define-XML are generated through
the public project submission API.

## Supported dependency combinations

The Rust workspace declares MSRV 1.82 and builds with Rust 1.99.1, the
toolchain that `rust/rust-toolchain.toml` pins; CI lints every crate but the
R binding with Rust 1.82.0 on every pull request, a release lane checks and
lints the whole workspace and builds and checks the R source package with
it, native qualification builds both source archives on Linux with it,
installed from its standalone installer, and a weekly lane lints and tests
it with the beta toolchain, the next stable release, before the pin moves.
Rust 1.82.0 was released on 2024-10-17, so it is at least the two-year-old
Cargo that CRAN's guidance for packages using Rust asks to test with, on any
submission date (#3111). The source packages carry no toolchain file, so they
build with the caller's toolchain, whose Cargo and rustc must each be 1.82 or
later: Windows R builds need the GNU target on that toolchain,
`rustup target add x86_64-pc-windows-gnu`. `make check-repository`
refuses a locked package that a supported target compiles and that needs a
newer Rust. Canonical release builds use `rust/Cargo.lock`: each source
package carries the part of it that its bundle builds (#3030 D55). Arrow and
Parquet are supported at exactly 55.2.0, the newest
release that builds with Rust 1.82, with flatbuffers 25.12.19, so an embedder
whose graph holds Arrow 55.2.0 builds the engine unchanged. Other Arrow and
Parquet releases are unsupported: the adapter's version-specific APIs, confined
to `storage_api.rs`, are adapted and qualified for 55.2.0 only. Broadening that range requires separate adapter and
installed-host qualification first. The direct storage codec floors are brotli
8.0.4, lz4_flex 0.11.6, zstd 0.13.3 and base64 0.22.1. Portable libm remains
exactly 0.2.16. The reviewed
saphyr-parser 0.2.1 with its compact-flow fix is vendored as
`yamaa-saphyr-parser`, which `yamaa-adapters` depends on by path. It is
therefore part of every graph that builds the engine, including a project that
depends on the facade by git or path with no other configuration; the Release
compatibility workflow's minimum-toolchain job builds the facade from such an
outside workspace (#2929). The Python
binding builds with PyO3 0.27, the newest release that builds with Rust 1.82;
the lock and the minimum dependency graph select 0.27.2.

The engine links one version of each crate. yamaa's own requirements on
lz4_flex, zstd, base64 and num-bigint stay in the series that Parquet 55.2.0
and the num crate of the Arrow crates require (0.11, 0.13, 0.22 and 0.4), so
each resolves to the release that Parquet uses (#3696): no package builds
Parquet any more, but the adapters' tests compile it as their differential
oracle (#3878). num-bigint's floor,
0.4.8, carries the division fix that 0.4.7 lacks. The duplicate versions that
`cargo tree -d` still lists add no second linked copy: syn serves procedural
macros, and the extra getrandom version serves a procedural macro too.

yamaa names flate2's zlib-rs backend beside its rust_backend (miniz_oxide),
as Parquet 55 did while the engine built it, and flate2 prefers zlib-rs, so
the engine's GZIP decoding, for Parquet page admission and ODM archives, uses
zlib-rs (#3878). zlib-rs is pure Rust under the Zlib license, which the
license notices now accept.

Python supports CPython 3.12 through 3.15. The package has no runtime
dependency, and on CPython 3.12 to 3.14 the declared floors of the `test`
extra in `python/pyproject.toml` are
lxml 6.1.3, pandas 2.3.3, Polars 1.35.2, py-yaml12 0.2, PyArrow 23.0.1,
Pydantic 2.12.4, packaging 25 and PyYAML 6.0.3.
The Parquet writer, which saves the bytes that parquet 55.2.0 saved (#3878),
records only type-defined column orders, which
Polars 1.35.2 reads; the IEEE 754 total-order form that later writers record
for floating-point columns needs Polars 1.43.2. R supports R 4.2.0 and later
and uses the shared Rust graph; its host qualification readers are the
`Suggests` dependencies in `R/yamaa/DESCRIPTION`.

Four of these floors do not work on CPython 3.15, so on 3.15 the declared
floors are the oldest releases that do: pandas 3.0.6, Polars 2.0.0, PyArrow
26.0.0 and Pydantic 2.14.0. Environment markers split these four requirements
by interpreter, and CPython 3.12 to 3.14 keep the floors that embedders hold.
pandas 2.3.3 and 3.0.6 are the first releases with wheels for CPython 3.14 and
3.15.
Polars before 2.0.0 installs, but its Series methods that forward to
expressions, such as `implode`, return `None`: 3.15 no longer stores a
function's implicit `None` return in `co_consts`, which those releases use to
find the methods. PyArrow 23.0.1 and pydantic-core 2.41.5, which Pydantic
2.12.4 requires, publish no 3.15 wheels, and their source builds need Arrow C++
or a PyO3 release that accepts 3.15. PyYAML publishes no 3.15 wheels either,
but builds from its source archive, so its floor is the same on every
interpreter. So is lxml's: 6.1.3 is the first release that loads no external
parameter entities by default, and 6.1.1 patched the libxslt that its Linux
wheels bundle.

The [dependency graph lanes](../CONTRIBUTING.md#ci-and-release-qualification)
test declared direct minima and newest compatible selections on Linux with
CPython 3.12, 3.14 and 3.15, all with R 4.6.1; on 3.15 the minima are its own
floors. Cargo transitive dependencies use the newest
MSRV-compatible resolution in both lanes; vendored crates and exact pins are
preserved. Python runtime/test floors are exact in the minimum lane, while
Python transitives are resolved compatibly. These isolated graphs do not change
`rust/Cargo.lock` or `python/uv.lock`. The full native matrix separately qualifies
the locked release across supported platforms and Python versions.

The same workflow checks both locks against security advisories.
`cargo deny check advisories` reads `rust/Cargo.lock` against RustSec with
`rust/deny.toml`, `pip-audit` reads every package locked in `python/uv.lock`
against the PyPI advisory database, and OSV-Scanner reads both locks against
OSV with `osv-scanner.toml`. OSV also carries GitHub's reviewed advisories,
which the other two databases can lack. An ignored advisory names its id, a
reason and a review date in the file of each scanner that reports it. An
OSV-Scanner ignore lapses on its review date, so the check fails again until
the advisory is reviewed. The Harbor evaluation's packages ship in no release
and are locked apart, in `evaluations/harbor/uv.lock`; the Harbor workflow
checks that lock with pip-audit and OSV-Scanner, so an advisory there does
not fail this lane.

### Components outside the advisory scanners

The scanners match locked registry packages by name and version, so they do
not cover vendored source or native libraries bundled inside a package. These
components are tracked by hand:

- `saphyr-parser` 0.2.1 with the reviewed compact-flow patch and 0.2.2's raw
  NUL fix, vendored as `yamaa-saphyr-parser`, a path dependency in
  `rust/vendor/saphyr-parser`. It stays vendored for 0.6.0; #3682 holds the
  upstream pull request and any later swap to a published release (#3030
  D229).
- The ReadStat 1.1.9 SAS reader subset in
  `rust/crates/yamaa-adapters/vendor/readstat`, with the later upstream fixes
  that its README lists through the review baseline it records.
- zstd 1.5.7, the C library that `zstd-sys` bundles into the engine.
- libxml2 and libxslt, bundled in the `lxml` wheels. The lxml 6.1.3 Windows
  wheels bundle libxml2 2.11.9, an end-of-life branch that lxml describes as
  security-patched.

The first three are also rows of the component inventory that every package
ships, with the versions and sources that their provenance records give
(see [source package contents](#source-package-contents)).

## Release checklist

Before pushing the release tag, confirm on the final commit:

1. Its complete native qualification passed.
2. Its Dependency graphs run passed, including the three advisory checks.
3. It passes the [interface check](#stability-contract) against the previous
   final release. The check never enforces major 0, so for 0.6.0 it ran
   before the release identity moved to 0.6.0, and the 0.6.0 notes summarize
   the breaks against `yamaa-v0.5.0` that it listed (#3462, #3706). For
   1.0.0 it lists the breaks against `yamaa-v0.6.0`, for the 1.0.0 notes.
4. Every ignored advisory in `rust/deny.toml` and `osv-scanner.toml` still
   matches a locked package, and its reason still holds. Remove any ignore
   that fails either test. An ignore past its review date fails the OSV
   check until it is reviewed, so the first release after each date
   re-reviews it and dates it again or removes it (#3154). After 2027-01-31,
   that release re-reviews PyO3 0.27.2's RUSTSEC-2026-0176 and
   RUSTSEC-2026-0177, checking that the Python binding still iterates lists
   and tuples only with `next` and never calls `PyCFunction::new_closure`,
   and thrift 0.17.0's GHSA-2f9f-gq7v-9h6m, checking that admission still
   walks and bounds every footer and page header before Parquet decodes
   them. After 2027-04-30, it re-reviews paste's RUSTSEC-2024-0436, checking
   that paste is still only a compile-time dependency.
5. Every [component outside the advisory scanners](#components-outside-the-advisory-scanners)
   was compared with its upstream releases and security advisories since the
   previous release, and each fix that applies is taken or has an issue.
6. The vendored ReadStat was compared with upstream `dev` commit by commit.
   Each commit after the review baseline that its
   [README](../rust/crates/yamaa-adapters/vendor/readstat/README.md#upstream-changes-after-119)
   records, and that changes a vendored file, is ported or listed there as not
   ported with its reason, and the README records the new baseline. In a
   ReadStat checkout, `git log <baseline>..origin/dev -- <vendored files>`
   lists them, with each vendored file named by its path under `src/`.
7. A [release rehearsal](#publication) passed against a native run of that
   commit; the tag names that run.
8. For a release candidate, `docs/releases/X.Y.Z-rc.N.md` says what changed
   since the previous candidate, or for candidate 1 what the candidate is, and
   the rehearsal shows the release body that the tag will publish.

Record the outcome on the release's planning issue.

After the tag, announce the release only once its Python packages are on
PyPI and the [PyPI check and the PyPI install test](#pypi-upload) have passed.
Record those outcomes on the planning issue too.

## Publication

After the final commit is on main and its complete native qualification has
passed, push the release tag at that exact commit: `yamaa-vX.Y.Z` for a final
release, or `yamaa-vX.Y.Z-rc.N` for release candidate N, as the
[release identity](#supported-platforms) names it. The tag must be annotated,
and its message must end with a trailer paragraph that names the qualifying
native run as `Qualified-Run: <run id>`. The run id is the number at the end of
the run's URL, also listed by

```sh
gh run list --repo elong0527/yamaa --workflow native.yml --commit <commit> --status success
```

Tag and push, with `<tag>` such as `yamaa-v0.6.0`:

```sh
git tag -a <tag> <commit> -m "yamaa <version>" -m "Qualified-Run: <run id>"
git push origin refs/tags/<tag>
```

`.github/workflows/release.yml` publishes from the named run alone, so a later
run of the same commit cannot replace the run the release was reviewed against.
The named run must be a completed successful native run of the peeled tag
commit: a scheduled main run, a main dispatch, or a same-repository
`qualification/issue-*` push. It requires all installed lanes, the offline
wheel and source installs, the offline toolchain route, the R binary
installs and the unpacked command-line archives among them, the R binary,
command-line and conformance archive builds, shared Rust/math, complete cohort
comparisons, strict R package
checks and CRAN's incoming checks to have passed. Branch dispatches cannot release. A tag other than the one the release
identity names fails before any request: a candidate tag on a final release, a
final release tag on a candidate, or another version or candidate number. So
does a lightweight tag or a missing, repeated or malformed trailer, and a named
run that does not qualify fails before anything is downloaded.

The workflow downloads the qualified artifacts and publishes their unchanged
bytes. `tools/ci/release_assets.py` declares the release assets once; the
selector, the publisher, the release rehearsal and the
[PyPI check](#pypi-upload) read that declaration, and a test keeps this table
equal to it:

| Asset | File | Native artifact |
| --- | --- | --- |
| Python wheel for Linux x86_64 | `yamaa-<Python version>-cp312-abi3-manylinux_2_28_x86_64.whl` | `native-python-ubuntu-24.04-3.14` |
| Python wheel for macOS arm64 | `yamaa-<Python version>-cp312-abi3-macosx_*_arm64.whl` | `native-python-macos-15-3.14` |
| Python wheel for Windows x64 | `yamaa-<Python version>-cp312-abi3-win_amd64.whl` | `native-python-windows-2025-3.14` |
| Python wheel for Linux aarch64 | `yamaa-<Python version>-cp312-abi3-manylinux_2_28_aarch64.whl` | `native-python-ubuntu-24.04-arm-3.14` |
| Python source archive | `yamaa-<Python version>.tar.gz` | `native-python-ubuntu-24.04-3.14` |
| R source package | `yamaa_<R version>.tar.gz` | `native-r-ubuntu-24.04` |
| R binary package for macOS arm64, R 4.6 | `yamaa_<R version>-4.6.tgz` | `native-r-binary-macos-15-4.6.1` |
| R binary package for macOS arm64, R 4.5 | `yamaa_<R version>-4.5.tgz` | `native-r-binary-macos-15-4.5.3` |
| R binary package for Windows x64, R 4.6 | `yamaa_<R version>-4.6.zip` | `native-r-binary-windows-2025-4.6.1` |
| R binary package for Windows x64, R 4.5 | `yamaa_<R version>-4.5.zip` | `native-r-binary-windows-2025-4.5.3` |
| Command-line binary for Linux x86_64 | `yamaa-<Cargo version>-x86_64-unknown-linux-musl.zip` | `native-cli-x86_64-unknown-linux-musl` |
| Command-line binary for macOS arm64 | `yamaa-<Cargo version>-aarch64-apple-darwin.zip` | `native-cli-aarch64-apple-darwin` |
| Command-line binary for Windows x64 | `yamaa-<Cargo version>-x86_64-pc-windows-msvc.zip` | `native-cli-x86_64-pc-windows-msvc` |
| Command-line binary for Linux aarch64 | `yamaa-<Cargo version>-aarch64-unknown-linux-musl.zip` | `native-cli-aarch64-unknown-linux-musl` |
| Grammars and expression evaluation set | `yamaa-<Cargo version>-conformance.zip` | `native-conformance-ubuntu-24.04` |

`R CMD INSTALL --build` names a binary package `yamaa_<R version>.zip` on
Windows and `yamaa_<R version>.tgz` on macOS. The two R series' binaries of
one platform go in one release, so their asset names add the series after a
hyphen. `install.packages(path, repos = NULL)` takes the package name from
what comes before the underscore, and the version from the package itself,
so it installs these files as they are. The selector also requires each
binary's `Built` field to name the R version and platform of its lane.

A command-line archive's name gives the engine's version, in the Cargo
spelling that `yamaa --version` prints, so a candidate's reads
`1.0.0-rc.1`, and the Rust target of its binary. The archive holds one
directory of the same name, with the `yamaa` executable, `yamaa.exe` on
Windows, and the package notices. The selector requires exactly those files:
the executable with Unix mode 0755 and a header that names its target's
format and architecture, so that `unzip` gives an executable that runs, and
the notices with mode 0644, unchanged.

The conformance archive's name gives the release's version in the same
Cargo spelling, which its tag and title use. It holds one directory of the
same name, with the closed grammars, `yaml/grammar/expression.yaml` and
`yaml/grammar/string-template.yaml`, the README that states their file format
and shape notation, `yaml/grammar/README.md`, the evaluation set of the
expression language, `yaml/conformance/expressions.yaml`, and the project
license, `LICENSE`, each at its path in the repository (#3690). Another
implementation can check itself against the grammars' vectors and the
evaluation cases of the release that it names. The selector requires
exactly those files, each with Unix mode 0644 and the bytes of its copy in
the checkout, so a release publishes them as committed at its tag. The
files are the same for every platform, so the source platform's
`native-conformance` job builds the archive once.

Each native artifact holds exactly the assets listed for it, so a release
publishes every asset that the native run built, or nothing: a missing,
ambiguous, undeclared or expired file fails publication, and nothing is
rebuilt. Every native source lane, on Linux, macOS and Windows, installs and
tests the two Linux-built source archives; the other hosts build a Python
source archive only to check it. An existing release is never overwritten.
Re-running failed jobs or all jobs keeps a run eligible: earlier attempts'
artifacts stay on the run, and the workflow takes only the copy that its
uploading job created during the execution the run's final attempt lists.

The release body of a final release X.Y.Z is its committed notes,
`docs/releases/X.Y.Z.md`, then a "Qualified native artifacts" section that
links the native run attempt and lists the selected artifacts. A release
candidate is created as a GitHub pre-release. Its body first says that it is
release candidate N of X.Y.Z and not the final release, where to report
feedback, how to install it from PyPI, and that the R package keeps version
X.Y.Z, so R users reinstall it when X.Y.Z is released. Then comes the
committed `docs/releases/X.Y.Z-rc.N.md`: for candidate 1 under the heading
"Release candidate 1", and for a later candidate as the changes since the
previous candidate, with a link that compares the two tags. Then come the
notes of X.Y.Z so far and the native evidence. Candidate N follows
candidate N-1: that candidate's tag must exist and be an ancestor of the tag
commit. A body longer than GitHub's limit of 125,000 characters fails before
the downloads.

The GitHub release uses the workflow's GitHub token, and the workflow
publishes nothing else: the owner then uploads the release's Python packages
to PyPI from its assets, as [PyPI upload](#pypi-upload) describes.

The `Release rehearsal` workflow runs the same selection and downloads, on
dispatch, against the latest successful native run on main, with its own
read-only token. It reads no tag, does not look for an existing release and
stops before `gh release create`. Its summary lists the assets and shows the
release body that a tag at that commit would publish, a candidate's
introduction included. Dispatch it before tagging with
`gh workflow run release-rehearsal.yml --repo elong0527/yamaa`; its log names
the run it rehearsed. To rehearse another run, check out that run's commit,
fetch `origin/main`, and run the selector with an authenticated GitHub CLI:

```sh
python -m tools.ci.select_release_artifacts --repository elong0527/yamaa \
  --run-id <run id> --destination <new directory>
```

### PyPI upload

From 0.5.0 on, PyPI receives each release's Python packages (#3030 D177):
the wheels and the Python source archive of the asset table above. No other
asset goes to PyPI: the R source and binary packages stay GitHub release
assets, and the source package goes to CRAN too for a final release, as
[CRAN submission](#cran-submission) describes. No workflow holds a PyPI
credential: once `release.yml` has created the GitHub release, the owner
uploads its Python packages with the owner's own PyPI credentials
(#3030 D57). Nothing is rebuilt, so PyPI serves the bytes that native
qualification tested.

Download the wheels and the source archive by asset name from the GitHub
release that the tag created, into a new directory:

```sh
gh release download <tag> --repo elong0527/yamaa --dir <new directory> \
  --pattern 'yamaa-<Python version>-*.whl' \
  --pattern 'yamaa-<Python version>.tar.gz'
```

Upload those files unchanged with twine, which asks for the owner's PyPI API
token or reads it from `TWINE_PASSWORD`:

```sh
uvx twine@latest upload <new directory>/*
```

Without `@latest`, uvx reuses a twine cached by an earlier run or installed
with `uv tool install`, and an older twine can refuse the wheels' metadata
version, 2.4.

A candidate needs no option: its version, X.Y.ZrcN, makes it a pre-release on
PyPI. PyPI refuses a file name that it already holds, so if an upload stops
partway, run it again with `--skip-existing`.

Then check what PyPI serves, from a checkout of the tag with an authenticated
GitHub CLI:

```sh
python -m tools.ci.check_pypi_release --repository elong0527/yamaa --tag <tag>
```

The check downloads each file that PyPI's JSON API lists for the release's
Python version, and the asset of the same name from the GitHub release, and
compares their bytes. It fails when a file differs, when PyPI lacks one of the
release's Python packages or serves any other file, and when the release does
not hold exactly the assets that the tag's `tools/ci/release_assets.py`
declares. It compares the bytes themselves and ignores the digests that PyPI
and GitHub list.

Then install the release from PyPI as its users do, with the `PyPI install`
workflow:

```sh
gh workflow run pypi-install.yml --repo elong0527/yamaa -f version=<Python version>
```

Each of its jobs installs `yamaa==<Python version>` with pip into a fresh
virtual environment outside any checkout, with `--pre` for a candidate, as the
candidate's release body installs it. Its wheel jobs run on every platform
that the release has a wheel for, with the oldest and the newest supported
CPython, and pass `--only-binary=:all:`, so pip installs a wheel and never
builds a source archive. Its source job builds the source archive from PyPI
with `--no-binary yamaa` on Linux x86_64, with the runner's Rust. Each job then
checks the installed version and runs `yamaa.selftest()`, whose report the log
keeps, and fails unless the selftest reports `succeeded`. Announce the release
only once every job has passed. With `-f index=testpypi` the workflow
rehearses an upload to TestPyPI: its wheel jobs install yamaa from TestPyPI
and its dependencies from PyPI, and its source job is skipped.

A final release is then installed with `pip install yamaa`. A candidate's
version is a PyPI pre-release, which a bare `pip install yamaa` does not
install reliably, so its release body gives
`pip install --pre yamaa==X.Y.ZrcN`. PyPI holds 0.5.0, whose wheels and Python
source archive were uploaded from its GitHub release assets unchanged
(#3030 D177). 0.6.0 follows it there, with no candidate before it
(#3673 D236, D289), uploaded from the assets of the GitHub release that its
tag creates. The GitHub release assets remain the standalone download route
for every package.

### Command-line binaries

The command-line binaries reach users through the GitHub release alone, and
have no installer. A user unzips the archive of their platform and runs the
`yamaa` executable in its directory, or puts that directory on the `PATH`.
`yamaa --version` names the engine, and `yamaa selftest` runs the selftest
cases as the hosts' `selftest()` does.

The binaries ship unsigned and without notarization (#3030 D52), and each
system says so the first time:

- On macOS, a file downloaded with a browser carries the quarantine
  attribute, which Archive Utility passes on to the files that it unpacks,
  and Gatekeeper refuses to run a quarantined binary that is not notarized,
  saying that Apple could not verify it. Clearing the attribute once, in the
  archive's directory, lets the binary run:
  `xattr -d com.apple.quarantine yamaa`. An archive downloaded with `curl` and
  unpacked with `unzip` carries no attribute.
- On Windows, a file downloaded with a browser carries the mark of the web,
  which Explorer passes on to the files that it extracts, and SmartScreen can
  stop an unsigned executable that carries it with "Windows protected your
  PC": **More info**, then **Run anyway**, starts it. `Unblock-File` in
  PowerShell removes the mark, from the archive before extracting it or from
  `yamaa.exe`. Where application control admits only signed executables, the
  binary needs an administrator's approval, while the Python and R packages
  load inside interpreters that are already allowed.

## CRAN submission

CRAN receives final releases, never a candidate, the first being 0.6.0
(#3673 D289, which amends #3030 D1's "CRAN receives 1.0.0"). Before
submitting, the release's `yamaa_X.Y.Z.tar.gz` passes `R CMD check --as-cran`
on win-builder (R-devel) and the macOS builder, beside the qualifying native
run's CRAN checks, and `R/yamaa/cran-comments.md` records the results and
explains every remaining note. A release reaches its channels in this order:

1. The final release tag `yamaa-vX.Y.Z`, pushed as [Publication](#publication)
   describes.
2. The GitHub release that `release.yml` creates from the tag's qualified
   native run.
3. The Python packages on PyPI, which the owner uploads from that GitHub
   release's assets, as [PyPI upload](#pypi-upload) describes (#3030 D57).
4. The R source tarball on CRAN: the `yamaa_X.Y.Z.tar.gz` asset of that final
   GitHub release, unchanged, submitted through CRAN's web form by the
   maintainer that `DESCRIPTION` names. A candidate's R tarball carries the
   same version (#3030 D53) and is never submitted.

CRAN is not pre-empted: 0.6.0 is submitted whatever its tarball size and
build time, and only if CRAN refuses the full engine on size or build time
does R ship through the release assets until CRAN accepts it (#3030 D50,
D34). If CRAN asks for a change after the 0.6.0 tag, the change lands on main
and every host moves to a coordinated 0.6.1; the 0.6.0 GitHub release stays
as published (#3030 D51). After 1.0.0, such a patch is one that the stability
contract allows.

Each later release, such a 0.6.1 included, follows the same sequence after
the [release checklist](#release-checklist), whose item 4 names the dated
advisory re-reviews: the first release after 2027-01-31 re-reviews the PyO3
and thrift ignores, and the first after 2027-04-30 the paste ignore (#3154).

## Source package contents

Python and R source packages carry the same engine crates, static ReadStat
bridge and patched YAML parser. SAS and Transport decoding runs in the shared
adapter over captured source bytes; it needs no host reader fallback or dynamic
ReadStat installation. The [ReadStat integration](../rust/crates/yamaa-adapters/vendor/readstat/README.md)
and [YAML parser provenance](../rust/vendor/saphyr-parser/PROVENANCE.md)
own their source inventories, patches and licenses. Both parser authors' MIT
notices remain in the distributed `LICENSE-YAML.txt`.

`tools/staging/engine_bundle.py` stages each host's engine source bundle,
from which both source packages build (#3030 D55). The bundle holds the
engine crates `yamaa-core`, `yamaa-adapters`, `yamaa-application` and `yamaa` with `yamaa-boundary`
and that host's binding crate; the canonical workspace manifest with `crates/*` as its
members, which in the bundle are exactly those crates, and with the profiles
that the host builds with, so that the R bundle's leaves out
`[profile.dist]`, which only the wheels and the command-line binaries
select; a lock trimmed from
the canonical `rust/Cargo.lock`; the vendored YAML parser, without its tests,
and the ReadStat sources; and the crates.io crates of that lock, as
`cargo vendor --locked --versioned-dirs` writes them, apart from the files
that `tools/staging/vendor_crates.py` leaves out, described below. The
facade crate carries the selftest cases in its `selftest/` directory: the
files of the benchmarks that `benchmarks/selftest.yaml` names, which the
facade's build embeds (#3030 D108). The adapters crate carries the schema
files that it embeds in its `schema/` directory: the copies of the schema
modules and JSON Schemas of `yaml/` that
`tools/packaging/generate_shipped_schema.py` writes and
`make check-repository` compares. A build from the bundle thus reads nothing
outside it. It holds no tests, examples, other benchmark fixtures or
`public-api.txt`, and no `rust-toolchain.toml` or `rust-toolchain` file
anywhere, the vendored crates included. Cargo writes the trimmed lock from a
copy of the canonical one, and
the tool then verifies it crate by crate: every package that it keeps is the
canonical entry, with the same version, source and checksum, depends only on
packages that its canonical entry depends on, and is reached from the
bundle's members. `make check-repository` verifies both hosts' trimmed locks,
and `python -m tools.packaging.check_cargo_lock <archive>` checks a built
source archive of either host against the checkout: the bundle's files byte
for byte, its lock, each vendored crate with exactly the files that its
`.cargo-checksum.json` lists, and no toolchain file anywhere in the archive,
including the R package's compressed crates. Native qualification runs it on
every source archive that it builds, and `make test-r` on the R source
package that it checks.

Without a toolchain file, each package builds with the caller's default
toolchain, which must be Rust 1.82 or later. rustup reads a toolchain file in
the directory that a command runs in and in its parents, and Cargo runs a
crate's build script in that crate's directory, so a file that a vendored
crate publishes could make rustup select another toolchain there: a download
during installation, or a failure offline. The vendoring script therefore
removes every such file from the crates. The R build also sets
`RUSTUP_AUTO_INSTALL=0`, so rustup installs no toolchain even where a
`rust-toolchain.toml` in a parent directory names one.

The Python source archive is maturin's output over the bundle, staged beside
the Python package files; `python/pyproject.toml` names plain maturin as its
build backend. The archive keeps the vendored crates in `vendor/` and, at its
root, where pip runs the build, a `.cargo/config.toml` that replaces crates.io
with them and sets Cargo's `net.offline`. maturin runs Cargo with `--locked`,
so the build uses the trimmed lock unchanged and reads nothing from the
network. It builds the `dist` profile that `python/pyproject.toml` selects,
as the native workflow does for every wheel: release, without the symbol
table.

The R source package builds offline from the R bundle, as CRAN's Repository
Policy and its guidance for packages that use Rust ask.
`tools/staging/stage_r_package.py` stages the bundle's workspace as
`src/rust`, its crates.io crates as `src/rust/vendor.tar.xz`, a reproducible
xz-compressed tar that `tools/staging/vendor_crates.py` writes, and its Cargo
configuration as `src/rust/vendor-config.toml`. The archive is written when
the package is staged and never committed, and the package carries the script
as `tools/vendor_crates.py`.

The package's `configure` and `configure.win` run `tools/configure.R`, as
CRAN's guidance asks. It looks for `cargo` and `rustc` on the PATH and then in
`~/.cargo/bin` (or `$CARGO_HOME/bin`), requires each to be at least the
`rust-version` that the bundled `src/rust/Cargo.toml` declares, reports both
versions in the installation log before compilation starts, and otherwise
stops with installation advice. On Windows it also requires the GNU target.
It writes `src/Makevars` or `src/Makevars.win` from their `.in` templates with
the paths it checked, the target directory, and on macOS the deployment
target. It also appends a row for the Rust standard library, with the version
that `rustc` reports, to the component inventory that R installs (below), and
`cleanup` removes that row with the Makevars. The Makevars unpack the archive
below `src/rust/target`, give Cargo a home there whose configuration,
`src/rust/vendor-config.toml`, replaces crates.io with the unpacked crates,
and run
`cargo build --locked --offline -j 2` with
`MAKEFLAGS` cleared: Cargo otherwise joins the jobserver of a parallel make
(GNU make 4.4 passes it to every command) and runs as many jobs as make
allows. The installation therefore reads nothing from the network, runs at
most two jobs and writes nothing outside the build tree and the temporary
directory. The recipe passes the target directory to Cargo, so a
`CARGO_TARGET_DIR` set for other projects does not move the build. Only
`YAMAA_R_CARGO_TARGET_DIR`, an absolute directory that `configure` reads,
moves Cargo's build, its home and the unpacked crates there, where the next
installation reuses them. CI's `r` job points it at the Rust cache that the job
restores, so the installation in `R CMD check` reuses the build of the one
before it; CRAN, users and the native R lanes leave it unset. Unset, the
recipe also keeps Cargo's build directory in the target directory and runs no
rustc wrapper, whatever the environment or a `.cargo/config.toml` in a parent
directory sets (`CARGO_BUILD_BUILD_DIR` or `build.build-dir`, and
`RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER` or their `build.` keys). The Makevars
build the release profile, which strips nothing, and override none of R's
link flags: CRAN's Repository Policy asks a package not to remove diagnostic
information such as symbols in shared objects. The shared object therefore
keeps its symbols on Linux and macOS. On Windows, R's own toolchain links
every package DLL with `-s`, also in the binary packages that CRAN and the
release build, so the Windows `yamaa.dll` carries no symbol table; the flag
is R's, not the package's (#3030 D140). The package's
`src/rust/Cargo.toml` leaves out the wheels' stripping `dist` profile, so it
names no stripping, and `check_cargo_lock` refuses an R source package whose
Makevars or Cargo profile would strip.

The vendoring script keeps whole only the crates that a build for one of the
supported targets (`check_minimum_rust.TARGETS`) compiles, as Cargo resolves
them for each target: the crates that the target links, with their procedural
macros and their build scripts' dependencies (#3368). Each other crate, such
as `r-efi`, `windows-core` and the WebAssembly and WASI crates, keeps only its
manifest, which Cargo reads to resolve the lock. Where the manifest leaves its
library to Cargo's discovery of `src/lib.rs`, an empty `src/lib.rs` takes the
place of the crate's, so neither package carries any of those crates' code.
The script then checks that Cargo resolves the lock offline from what remains.
It also leaves out the license texts that a crate ships for an alternative to
the license that yamaa relies on, which `ALTERNATIVE_LICENSES` lists: zstd's
`COPYING`, the text of the license that zstd offers beside BSD-3-Clause (#3030
D136). CRAN's policy admits no binary executable code in a source package, and
neither package carries any: the prebuilt WebAssembly objects of `wit-bindgen`,
a crate that only wasm32 targets build, leave with the rest of that crate, and
a binary in a crate that a supported target compiles stops vendoring. Cargo's
`.cargo-checksum.json` files stay as Cargo wrote them, apart from the entries
of the files left out, and do not list an empty `src/lib.rs` that the script
writes; the package checksum
stays too. Like the checksums in `Cargo.lock`, they belong to the packaging
tool, and the script computes no digest of its own. `inst/AUTHORS`, written
at staging from `cargo metadata`, gives every crate that the R package
bundles its declared authors, license and repository, and names the files
left out; `DESCRIPTION` credits those authors, and the authors of the bundled
ReadStat C sources, as contributors and copyright holders.

The R source package carries a small base-R smoke suite in `tests/`, with
only its project study. It runs the embedded format selftest and checks the
public R specification/project API, R callbacks and saved bytes. `R CMD
check` copies that directory and runs it without suggested packages; R
never installs it (#3105). Its `inst/` holds `AUTHORS`, the license notices
and the component inventory.

The full R regression suites and fixtures remain in the repository. `make
test-r`, pull-request CI and native qualification stage them outside the
checkout and run them against the installed package, separately from the
CRAN smoke checks. Each third-party SAS fixture keeps its license beside
it. `tools/staging/stage_r_package.py` lists the staged inputs that no full
R suite reads, which that staging leaves out.

The package also carries a short introductory vignette,
`vignette("yamaa-cran", package = "yamaa")`, linking the full site and
showing one self-contained public R example. Source-package builds render
it with knitr, rmarkdown and Pandoc; its example is not evaluated while
rendering. These build tools are not runtime requirements.

`tools/staging/vendor_exclusions.toml` records the audited development
directories that source bundles omit from specific crate versions. The
vendoring helper preserves license files and declared production inputs,
and the policy keeps embedded documentation sources such as winnow's
examples. An upgraded or new crate keeps its development files until reviewed.
`inst/AUTHORS` records the omissions, and the R source package carries the
helper and its policy together under `tools/`.

Every package carries the project license and the third-party notices: each
wheel in its `.dist-info/licenses/` directory, the Python source archive at its
root, the R source package in `inst/`, which R installs at the package
root, each R binary package at that root, and each command-line archive
beside the executable. `LICENSE.txt` is the project's MIT license.
`LICENSE-THIRD-PARTY.txt` lists the third-party crates that its host's crate
links: its normal dependency closure on each supported target, with the
features that the host's release build enables, without procedural macros.
Crates that only build scripts or procedural macros use, and crates that only
other features or other platforms select, are not linked and are not listed;
the source packages carry the license files of those that a supported target
compiles with their vendored sources, and only the manifests of the others.
Each entry names the one license that yamaa relies on, never the choice that
the crate offers, and reproduces the license, copyright and NOTICE texts that
the crate ships for it (#3364).
The Python packages' copy covers the Python binding's
crates, the R package's copy the R binding's, all of which the R source package
bundles, and the command line's copy, `rust/crates/yamaa-cli/LICENSE-THIRD-PARTY.txt`,
the crates of `yamaa-cli`. It also gives the standard library's licensing,
because the native code links that library: from the notice that the Rust
project ships with the pinned toolchain, `share/doc/rust/COPYRIGHT-library.html`,
the licensing statement and the in-tree licenses and copyright holders; and
from the toolchain's `rust-src` component, which that notice does not fully
cover, the third-party crates that the standard library links on the supported
targets, such as `addr2line` and `object`, each with its one license and the
texts that its package ships (#3369). From the same component, it also lists
the in-tree crates that the standard library links whose manifests declare a
license that the notice does not give them: `compiler_builtins`, a port of
LLVM's compiler-rt with the code of libm, relied on under MIT AND Apache-2.0
WITH LLVM-exception, with the license files of its sources (#3494). No
package notice names a GNU license;
the only GPL wording that remains is the clause of the LLVM exception to
Apache-2.0, a permissive license, which names GPLv2. The command line keeps
only that file, its component inventory and `LICENSE-MUSL.txt` beside its
crate; its archives carry the Python packages' copies of the other notices.
The archives for the musl targets, whose executables link the musl C library
statically, also carry `LICENSE-MUSL.txt`: musl's `COPYRIGHT` file, relied on
under MIT. `LICENSE-READSTAT.txt`,
`LICENSE-UNICODE.txt` and `LICENSE-YAML.txt` cover the vendored ReadStat
sources, the Unicode data in yamaa's own tables and the vendored YAML parser.
The R package keeps its `MIT + file LICENSE` declaration, and its `Copyright`
field names these files. The Python License-Expression,
`MIT AND Apache-2.0 AND BSD-3-Clause AND Unicode-3.0 AND Apache-2.0 WITH LLVM-exception AND Zlib`,
covers the project, the crates linked into the wheels and the standard
library, which is relied on under MIT, with Unicode-3.0 for the Unicode tables
in `core`, and the crates that the standard library links, of which
`compiler_builtins` adds Apache-2.0 WITH LLVM-exception (#3030 D185).

`tools.packaging.generate_third_party_notices` generates each host's crate
notices from `rust/Cargo.lock`, with the crates that `cargo tree` resolves
for the host's release build (#3470): the wheels build with the features
that `python/pyproject.toml` gives maturin, and the R package and the
command-line binaries with their bindings' default features. It generates the
standard library section from the
toolchain that `rust/rust-toolchain.toml` pins, its notice and its `rust-src`
component, which the repository checks add to it, and it derives that
expression from the Python binding's linked crates and the crates that the
standard library links. The repository check
fails when a host gains or loses a linked crate without a matching notice
entry in its host's copy, when `rustc` in `rust/` is not the pinned toolchain,
when the committed section differs from that toolchain's notice and sources,
when the musl notice was reviewed for another toolchain, and when a notice
would name a GNU license. Staging the R source package fails while its
notices name a crate that it does not bundle. Native qualification and the
release selector require every wheel, source package, R binary package and
command-line archive to carry its own host's notices unchanged, and refuse a
package with any text that names a GNU license, the files inside the R source
package's `vendor.tar.xz` included. The exceptions are the unchanged
upstream files of the vendored crates that
`tools/packaging/check_package_notices.py` lists with its reasons: zstd's
dual-licensed sources, the manifest of `r-efi`, and README or changelog
prose; and an R binary package's namespace loader, `yamaa/R/yamaa`, which R
writes itself from its own `share/R/nspackloader.R`, whose header names the
GPL that R is under.

Every package also carries `COMPONENTS.tsv` beside the notices, the component
inventory of its host: the wheels and the Python source archive carry the
Python inventory, the R source and binary packages the R inventory, which R
installs with the notices, and the command-line archives the command line's,
`rust/crates/yamaa-cli/COMPONENTS.tsv`. It lists every package of the host's
trimmed lock, including the crates that only build scripts and procedural macros use and
those that no supported target compiles, and the C code that those crates
compile from sources that they carry: ReadStat in `yamaa-adapters` and the
zstd library that `zstd-sys` bundles. The inventory of the wheels and of
the command-line binaries also lists `rust-std`, the Rust standard library
of the pinned toolchain that builds them, which they link. The command
line's also lists musl, the C library that the pinned toolchain bundles for
the musl targets and that only the executables for those targets link, at
the release that `MUSL` pairs with that toolchain (#3493, #3030 D168). A
source archive links the standard library of
whatever toolchain builds it, so the Python source archive carries the
wheels' inventory without that row, and the R source package's inventory has
none. Instead, the R package's `configure`, which already runs
`rustc --version`, appends one `rust-std` row with the version that it
reports to the copy in `inst/` that R installs, after the other rows, and
`cleanup` removes it, so that `R CMD build` ships the inventory as committed.
Every R installation therefore names the standard library that it linked: an
installation from source names the toolchain that built it, and the R binary
packages, which the pinned toolchain builds, name that one. Each
inventory is tab-separated text with a header row and one row per component,
giving its name and version; its license, the one license that yamaa relies
on of those that the crate's manifest declares, or that it relies on for the
C code (`BSD-3-Clause` for zstd) or the standard library (`MIT`); its source, `crates.io` or
the component's directory in this repository, with the upstream repository and
commit of vendored code, the crate that bundles it, or the Rust repository at
the release's tag; and its purpose, from `tools/packaging/crate_purposes.toml`
for a crate or from `tools/packaging/generate_component_inventory.py` for the
C code and the standard library. It records no digests. Packages that pip or
R install beside yamaa, such as Polars, are not among its components.
`tools.packaging.generate_component_inventory` writes the three inventories, and
the repository check and native qualification fail while one differs from
what its host's trimmed lock, the crates' manifests, the purposes, the
vendored sources' provenance and the pinned toolchain give. The package
notice checks require each package to carry its own host's inventory
unchanged, apart from the standard library row: the Python source archive's
leaves it out, and an R binary package's adds exactly the one that
`configure` appends for the pinned toolchain. The repository check also
fails while a host's notices list other crates than its inventory marks as
linked: a crate whose purpose starts with `build script:`,
`procedural macro:` or `not compiled:` is one that no build links (#3470).

Native qualification exercises the static source bridge on every supported
platform and compares both installed hosts with independently authored binary
fixtures and exact saved bytes. The [SAS source contract](../rules/reference/sas.md)
owns decoding behavior.

## Approved decisions

- Export Python `spec`, `project`, `read_odm`, `lint`, `schema`,
  `conditions`, `info`, `selftest`, `Table`, `Error` and `__version__`, and R
  `yamaa_spec`, `yamaa_project`, `yamaa_read_odm`, `yamaa_lint`,
  `yamaa_schema`, `yamaa_conditions`, `yamaa_info` and `yamaa_selftest`
  (#2967, #3700; #3030 D28, D40, D41, D149). A spec checks, resolves and runs
  one specification; a project selects its listed specifications by `domain`
  name, checks them all, runs one section or all sections, and composes
  submissions. Only results write files, through `save()`. `resolve()` returns
  the composed specification as a dict or named list. `lint()` and
  `yamaa lint` replace the style command line, `python -m yamaa.style`, which
  is gone with the Python package's `style` extra (#3176).
- Keep wrong argument types as `TypeError` in Python; in R they are ordinary
  errors. `check()` and `run()` return specification, data and project
  problems as issue rows, missing specifications included, and never raise for
  them; `save()`, `resolve()` and an unknown specification name or section
  raise `yamaa.Error` or a `yamaa_error` condition carrying `issues`. Check
  returns its findings without reading study data. Each project function is a
  `type: function` file that the project lists under `functions`, and the
  project's `environments` holds each host's lock (#3030 D8); no Python or R
  call takes a function file, a lock or a resource resolver.
- Return results as owned host tables: a `yamaa.Table` in Python, which needs
  no other package and converts to Polars, pandas or PyArrow (#3030 D11,
  D190), and a data frame in R, except a streamed run's output, which R gives
  as an Arrow C stream (#3697).
- Keep the Rust adapter features `yaml`, `parquet`, `readstat` and `odm`
  independent, all enabled by default; the Python and R packages enable all
  four.
- Preserve separate missingness and preserve-or-error conversions: Python
  keeps full signed-i64 values, and R uses plain doubles and character in
  output and in project function calls, raising on an integer beyond 2^53 or
  text containing NUL. R has no exact-value classes.
- Report every finding in one issues table, built in Rust and identical in
  both hosts: the twelve columns `severity`, `phase`, `condition`, `rule`,
  `spec_paths`, `file`, `line`, `column`, `message`, `context` as canonical
  JSON text, `source_span` and `operand_route` (#3030 D28). No host
  interprets conditions, and there are no separate diagnostic records.
- Give each run one run report, written to `output.report` (#2912).
- Resolve resources from their original declaring file by default; explicit
  input/output `relative_to: entry` selects each specification entry. Admit
  absolute and parent-relative local paths without project-root fallback. Preserve snapshots,
  original failures, callback counts and report-first publication order. A `schema`
  link types a stored artifact from its producing specification and never runs
  it; callers build and save producers before their consumers.
- Keep normative diagnostics registry-owned, including the reviewed
  `unsupported_operation` and `engine_rejected` null-rule exceptions.
  Cut breaks remain finite and strictly ascending; signed-i64 predicate
  overflow, CSV empty-string refusal, greatest/least first-source ties and
  row-filter behavior follow the normative rules and expected benchmark data.
- Use PortableLibmV1 and pinned libm 0.2.16. EXP/LN/POWER results, downstream
  predicates, conversions and saved bytes may differ from historical platform
  math. This promises neither universal correct rounding nor historical bit
  compatibility. The golden values live in
  [portable_math.tsv](../rust/crates/yamaa-core/tests/fixtures/portable_math.tsv).
- Build each source package from a per-host workspace whose lock is trimmed
  from the canonical `rust/Cargo.lock` and verified crate by crate against
  it, with vendored crates, no tests or fixtures, and no toolchain file
  anywhere, the vendored crates included (#3030 D55). Both source packages
  are built this way. Runtime, contract, cache and resource identities use
  references, canonical forms or bytes; only packaging-tool lockfiles and
  Cargo's records of vendored crates carry their own checksum fields.
- Prune every specification after composition, including an entry file that
  declares no `parent`; resolved-column ordering stays inheritance-only.
- Ship `str_replace` with literal replacement text only. Group references,
  if ever added, come through a different field, so `replacement` never
  changes meaning.
- Promote row defaults per template: a column can be row-phase for the
  templates that need it and column-phase for the rest, and a candidate a
  template filter suppresses never reads a lookup it does not need.
- Pin Arrow and Parquet to exactly 55.2.0, the newest release that builds
  with the minimum Rust (#3111), with brotli 8.0.4 and lz4_flex 0.11.6
  floors, so embedders on Arrow 55.2.0 build the engine unchanged. The 55.2.0
  writer's type-defined column orders keep the Python Polars floor at 1.35.2,
  with no vendored Parquet writer.
- Build from Rust 1.82, the release that CRAN's two-year-old cargo rule
  allows on any submission date, with edition 2021 (#3111). Keeping 1.85 and
  holding the CRAN submission until 2027-02-20 was rejected.
- Ship `saphyr-parser` 0.2.1 with the reviewed compact-flow fix, vendored as
  `yamaa-saphyr-parser`, in every supported graph. It stays vendored for
  0.6.0; #3682 holds the upstream pull request and any later swap to a
  published release (#3030 D229).
- Keep Cargo dependency floors where the minimum dependency-graph lane tests
  them; the crates are not published.
- Generate both hosts' binding declarations from
  `rust/interface/bindings.yaml` and compare them in the repository check
  and the native qualification lane. Both hosts run the version and feature
  handshake of `yamaa-boundary` before any other native call (#3679), and
  then call the `yamaa` facade they link (D281 on #3673).
- The `yamaa` facade crate is the only supported Rust surface; every other
  crate is internal and may change in any release, as the
  [stability contract](#stability-contract) says.
- Publish each release as GitHub release assets; the owner uploads its Python
  packages to PyPI from those assets (#3030 D13, D14, D57).
- Publish release candidates as GitHub pre-releases from `yamaa-vX.Y.Z-rc.N`
  tags, with Python version `X.Y.ZrcN` and Cargo version `X.Y.Z-rc.N`. The R
  package keeps `X.Y.Z` through the candidates, and each candidate's release
  body tells R users to reinstall the final release (#3030 D53).
- Read only the live records of a SAS7BDAT dataset that holds records
  deleted in place, and refuse it as `source_sas_invalid` with cause
  `deleted_records` when its count of deleted records and the records marked
  deleted disagree (#2924, #2927).
- A qualified `between.value`, in a named intermediate or an aggregate, reads
  the record of the input that drives the row. Any other input fails
  validation as `unknown_field`; it is never read through the implicit join
  (#2925).

The [R, Python and command-line API](articles/api.md) page describes host
behavior. The
[Python README](../python/README.md#installation)
and the
[R README](../R/yamaa/README.md#installation)
give the installation commands.

## Resource refusals

In the shared engine, a fixed resource boundary produces `engine_rejected` with
a null rule and its authored specification paths. The context retains the lifecycle stage, the
boundary's code, its resource name and integer limit, and the observed `actual`
size when available. Compiler boundaries use `compiler_limit`; physical capture,
preparation and execution boundaries retain their corresponding resource code
and phase. Static Check and Build share the same preparation facts.

A refusal stops that operation before its later effects. Earlier completed
findings and original source provenance remain available. Static Check retains
its metadata-only authority. Reduce the refused size and rerun the operation.

## Resource limits

The shared Rust engine behind every Python, R and command-line call uses one
fixed policy in `yamaa_core::limits::Limits`.
Admission checks the physical file, table geometry, declaration shape or compiled
regex at its owning boundary. There are no caller-selected profiles or
cumulative byte, copy, traversal or work pools.

`read_odm()` and `yamaa read-odm` keep their own archive-member,
expanded-byte and XML-depth options. A parser refusal raises `yamaa.Error`
in Python or a `yamaa_error` condition in R; `yamaa read-odm` prints the
refusal and exits with status 1. With `--format json`, it reports `status`
as `failed` and includes the refusal's `issues`.

| Boundary | Fixed maximum |
| --- | ---: |
| Each specification, schema, project, function, codelists or packaging-lock file | 8 MiB |
| Each encoded study source file | 10 GiB |
| Each saved output file | 10 GiB |
| Rows in a decoded or materialized table | 10,000,000 |
| Cells in a decoded or materialized table | 640,000,000 |
| Columns in a source, output or intermediate table | 1,024 |
| Keys in one key or grouping declaration | 64 |
| Inputs to one specification | 16 |
| Operands in one source selection | 64 |
| Links followed in one composition, each `parent` and each `include` entry | 8,192 |
| Compiled program of one regular expression | 10 MiB |
| Findings in the final public issue collection | 10,000 |
| YAML nesting depth | 64 |
| Semantic expression and context nesting depth | 128 |

Every Python, R and command-line build runs over key-bounded partitions
(`docs/partition-execution.md`, D264): partitions of at most 624,640 driver
records that never split a group of the specification's first two declared
keys, such as STUDYID and USUBJID, with the whole-table fallback. The row and
cell limits then bound one partition where the run divides a table, and the
whole table everywhere else:

| Table | What the row and cell limits bound |
| --- | --- |
| A CSV or Parquet driver input that is the specification's only input, in a run with no step that needs the whole table | Each partition, decoded one at a time |
| The output of such a run | Each partition, which holds at most as many rows as its largest driver partition; the run holds the whole output once, a column at a time, and a CSV or Parquet file is written one partition after another, with the whole-table write's bytes, row groups included |
| The run report | The whole report, built once from every partition's counts |
| A SAS, ODM or Dataset-JSON input, and its output | The whole table |
| Every input of a specification that reads more than one, such as the source of a lookup, join or named relation | Each whole table |
| A run with a step that needs the whole table, which the fallback runs whole | The whole input and output |
| A submission's SAS Transport or Dataset-JSON file | The whole file, whose header states its record count |

A step needs the whole table when what it reads crosses the partitions: a
window or aggregate whose grouping omits one of the two keys, a named or
joined relation, `SELF` rows or a project function. So a run whose steps
group by subject reads, builds and saves a CSV or Parquet input of any number
of records, and a run that needs the whole table refuses beyond the limits
exactly as it does without partitions. A Rust embedder's engine runs every
build whole unless it sets `EngineOptions::with_partitions`.

A MiB is 1,048,576 bytes; a GiB is 1,073,741,824 bytes. File readers check the
encoded size before retaining bytes and check growth during reading. A
compressed SAS dataset is also refused when the bytes its records decompress
to, its live records times its row length, exceed the source file limit. Writers
check the saved file size before extending it. Table admission checks rows,
columns and their product before constructing or growing the corresponding
geometry. A regular expression is refused when it compiles, never while it
matches: matching takes time linear in the subject, so there is no step budget.

Semantic admission and retained observations stay complete. The final public
issue collection keeps its first 10,000 findings in their existing order. Source
identity, path authority, encoding, scalar types, handle ownership and interrupt
propagation retain their own integrity checks. These fixed limits are independent
of available process memory.

## Keyed intermediate lookups

A keyed intermediate (`intermediates: [{id, dataset, key, columns}]`) is
served by a donor index built once per build attempt over the eligible
donor rows whenever the intermediate is plain: any number of key columns
and any scalar key type (`str`, `int`, `float`, `date` or `datetime`,
including an `int` key matched against a `float` donor column or the
reverse) are indexed over the full key tuple, and donor read order is
kept inside each key bucket. Lookup shapes that still scan the eligible
donor rows for every output row are intermediates with expression keys,
a range, derivations, a correlated filter or a `SELF` source, and any
lookup over a donor relation below the small-relation index threshold.
Indexed and scanned lookups keep the same diagnostics and ordering.

## Migration

From 0.5.0, the [0.6.0 release notes](releases/0.6.0.md#the-break-from-050)
state the break and link the rename table, which gives every changed
spelling; there is no migration guide and no converter (#3030 D35). For an
earlier release, see the notes of each release after it, such as the
[0.5.0 notes](releases/0.5.0.md).
