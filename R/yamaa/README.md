# yamaa R

The `yamaa` package builds, checks and renders submission metadata with the same
locked Rust engine as the Python package. It requires no Python runtime.

```r
library(yamaa)

result <- yamaa_domain("spec.yaml")
result$output
result$issues
result$verification_log
result$warning_log
result$save()
yamaa_check("spec.yaml")$issues

define <- yamaa_define("environment.yaml", "sdtm", created = "2026-09-11T00:00:00")
define$issues
define$save()
packageVersion("yamaa")
```

`yamaa_domain` computes without publishing files. Explicit Save publishes retained
producer bytes before consumers and reuses those bytes on repeated calls. A failed
build raises `yamaa_domain_error` on Save. Owned result attributes are read only;
issue context is JSON text. `yamaa_check` reads no study data or project code.
Resource paths resolve from their original declaring YAML layer under the
[local-path policy](../../rules/storage/resources.md).

Integer output uses `yamaa_int64_vector`, preserving the complete signed 64-bit
range and missingness separately. Ordinary text uses character vectors; columns
containing NUL use lossless `yamaa_utf8_vector`. Subsetting preserves the exact
values. Conversions preserve values or raise an error. Dates use Date and
datetimes use POSIXct with UTC.

`yamaa_define` admits submission metadata without executing a domain. Explicit
Dataset-JSON requests read already published artifacts. Save publishes retained
JSON in specification order and XML last, stopping after an operational failure.
A failed Define Save raises `yamaa_define_error`. See the
[shared Define contract](../../rust/DEFINE_GENERATION.md).

## Source installation

The approved first release channel is the standalone `yamaa_0.4.0.tar.gz` source
asset on a [qualified GitHub release](https://github.com/elong0527/yamaa/releases).
CRAN, r-universe and R binary distributions are later work. This checkout is still
a migration candidate; the release asset and evaluator removal require the
[transition gates](../../rust/planning/transition.md).

After downloading the tarball from the selected release:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
```

Supported source-build targets are R 4.6.1 on Linux x86_64, macOS arm64 and Windows
x64. Install Rust 1.90.0/Cargo and the platform's C compiler and GNU make first.
Windows builds require Rtools45 and the `x86_64-pc-windows-gnu` Rust target; see
[Windows R qualification](../../rust/R_WINDOWS.md). The archive contains the
complete Rust workspace and the unchanged canonical Cargo lock. Compilation uses
`cargo build --locked`; no checkout, Python runtime or new dependency resolution
is needed during R installation. Cargo downloads the already locked dependencies
when they are absent from its cache.

For a development build of the prepared 0.4.0 candidate, stage this repository
template before `R CMD build`.
Direct `devtools::install_github(..., subdir = "R/yamaa")` installation omits the
shared Rust sources and is unsupported. From the repository root:

```sh
python -m pip install py-yaml12==0.2.0
python rust/tools/stage_r_package.py /tmp/yamaa-stage/yamaa
R CMD build --no-build-vignettes --no-manual /tmp/yamaa-stage/yamaa
R CMD INSTALL yamaa_0.4.0.tar.gz
R CMD check --no-manual yamaa_0.4.0.tar.gz
Rscript --vanilla R/yamaa/tests/public_grammar.R
```

Staging refuses to overwrite an existing destination. The source archive includes
unchanged authored grammar documents and independent installed test inputs.
The native workflow rebuilds, installs and runs the public suites outside the
checkout, then requires `Status: OK` from `R CMD check` on all three supported R
platforms. See [Cargo locking](../../rust/CARGO_LOCKING.md).

The public API replaces the retired `cdiscbuilder` evaluator. Legacy SQL/semantic,
custom-schema/provenance and ODM authoring APIs, including `parse_odm_to_long_df`,
are outside the first release surface. Roll back a whole run by pinning the prior
qualified source release in `renv.lock` before execution. Existing published
outputs are retained; rollback does not undo earlier Save effects.
