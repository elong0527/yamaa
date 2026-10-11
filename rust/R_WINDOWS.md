# Windows R qualification

Windows R is required for the first shared Rust release under
[#1903](https://github.com/elong0527/yamaa/issues/1903). The native source-package
matrix includes Windows x86_64 with R 4.6.1, Rtools45 and Rust 1.90.0. Adding the
lane does not qualify a platform: its actual installed suites, strict package
check and complete public reports must pass on the reviewed source revision.
Full environment, producer, submission and release surfaces still require their
remaining implementation and acceptance gates.

## Native binding and build

The R binding exposes the shared Windows file preparation, capture, inspection,
project activation, producer execution, verification logs, shared Define and
explicit publication adapters. Its
registered owned handles and rooted host error/interrupt payloads use the same
implementation on Unix and Windows. The platform adapters retain their existing
path, file-kind, link and snapshot checks and implement the approved
[declaring-file local-path policy](../rules/storage/resources.md).

The official [Rtools45 build contract](https://cran.r-project.org/bin/windows/Rtools/rtools45/rtools.html)
covers R 4.5 and later, including R 4.6. Its x86_64 compiler uses GCC/MinGW-w64
and UCRT. `R/yamaa/src/Makevars.win` explicitly selects
`x86_64-pc-windows-gnu`, independently of rustup's host default. It builds the
R static library with the canonical Cargo lock, uses R's C compiler and archive
tool for native dependencies, and links the Windows system libraries. The
temporary `gcc_eh` discovery placeholder follows the
[upstream extendr template](https://github.com/extendr/rextendr/blob/main/inst/templates/Makevars.win.in)
and is removed with build intermediates after linking. `yamaa-win.def` exports
`R_init_yamaa`; all callable entry points are registered through the existing C
bridge and Rust initialization routine.

Install the additional target before building the staged package:

```sh
rustup toolchain install 1.90.0 --profile minimal
rustup target add --toolchain 1.90.0 x86_64-pc-windows-gnu
python rust/tools/stage_r_package.py C:/path/to/stage/yamaa
R CMD build --no-build-vignettes --no-manual C:/path/to/stage/yamaa
R CMD INSTALL --library=C:/path/to/empty/R-library yamaa_0.3.0.tar.gz
```

Use the Rtools45 tools on PATH for building. The source package retains all
workspace sources and the unchanged root manifest and Cargo lock. Runtime tests
require neither Cargo nor Python; the original-specification suite removes
Python from PATH before running the installed native API.

## Required evidence

The Windows R CI lane preserves checkout bytes and symlinks, checks the GNU
binding with strict Clippy, builds and installs a clean source package outside
the checkout, and runs the complete installed R supplemental catalog. That
includes original YAML, filesystem snapshots and publication, lossless scalars,
installed project functions, repeated activation, producer graphs, original
conditions/interrupts, retained results and failed-save gates. Declared log
sidecars must retain their original observations and Save order. Shared Define
must compare the unchanged DM XML/JSON goldens, admit AE/VS metadata without
executing specifications, retain held Save bytes after checked input deletion,
and exercise the real Windows case-alias REQ-1226 collision with unchanged
sentinels and no writes. Strict
`R CMD check` must finish with `Status: OK`; notes, warnings and errors fail the
lane.

The public cohort comparator requires Windows R evidence for every Windows
Python wheel/source cohort comparison. It consumes actual installed reports and
saved bytes through the existing independent M1 comparator. A missing R evidence
directory fails the job, and unsupported or unexercised runs cannot count as
passes. Historical missing-platform observations remain valid for their earlier
revisions until fresh evidence replaces them.

Artifacts retain the exact source revision, R/platform and Rtools compiler
identities, Rust host/target details, full locked Cargo metadata/features, strict
check logs, complete supplemental and public reports, the installed Windows DLL
and the tested source archive. Audits compare source and fixture bytes directly
against the reviewed revision. Cross-compilation or Rust Windows component tests
alone never establish installed R qualification. Windows arm64, other R versions
and binary-package distribution promises remain separate from this x86_64
source-build lane.

Both Python and R source archives must retain the native fixture manifest and
all fourteen original log/Define paths it declares. Audits compare those members
directly with the reviewed Git bytes, including the original CSV, XML and JSON
expected outputs. A changed or missing fixture cannot qualify a source package.
