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
issue context is JSON text. Compiler capacity refusals retain `engine_rejected`
and a null requirement, with `code: compiler_limit`, `stage: prepare`, the
`resource`, integer `limit` and counted `actual` in context. `spec_paths`
identifies the refused declaration; check and build return the same facts with
or without an environment. Numeric policy refusals retain their applicable
resource bound under `code: numeric_policy`. Layered preparation and YAML
resource refusals use `code: resource_limit` and retain the preparation or
decoder `phase` alongside the resource bound in context. `yamaa_check` reads no study data or project code.
Resource paths resolve from their original declaring YAML layer under the
[local-path policy](../../rules/storage/resources.md).

Inputs can use CSV, Parquet, SAS datasets (`.sas7bdat`), or SAS Transport v5/v8
(`.xpt`). SAS inputs explicitly declare `encoding`, for example
`{path: input/dm.xpt, encoding: windows-1252, types: {SEQ: int, VISITDT: date}}`.
Character fields default to `str`, numeric fields to `float`; numeric `types`
narrowing requires exact integral values. See the
[SAS source contract](../../rules/storage/sas.md) for missing values, character
padding, supported encodings and 1960 date/datetime epochs. Outputs remain CSV
or Parquet.

Integer output uses `yamaa_int64_vector`, preserving the complete signed 64-bit
range and missingness separately. Ordinary text uses character vectors; columns
containing NUL use lossless `yamaa_utf8_vector`. Subsetting preserves the exact
values. Conversions preserve values or raise an error. Dates use Date and
datetimes use POSIXct with UTC.

`yamaa_define` admits submission metadata without executing a domain. Explicit
Dataset-JSON requests read already published artifacts. Save publishes retained
JSON in specification order and XML last, stopping after an operational failure.
A failed Define Save raises `yamaa_define_error`. See the
[submission contracts](../../rules/README.md#submission).

## Implementation limits

The shared engine admits up to 1,024 declared columns, output columns and fields
in each input's `types`. CSV, Parquet, SAS, Arrow imports and producer contracts use
the same width ceiling. An `ordinal` adds one field and counts toward this bound.
Specifications still admit at most 64 keys and 16 inputs.

The public hosts use the longitudinal profile: 10,000,000 source or output
rows, 640,000,000 source or output cells, 1 GiB scalar text, 512 MiB output
text and 1 GiB encoded input/output. CSV admission includes its header in the
field and record quotas. Finite report and producer budgets account for the
same profile. Execution permits 2,000,000,000 logical work units; producer nodes
share cumulative allowances, and expensive plans can exhaust work before size.

Validation and normalization share 67,108,864 logical work units; complex or
inherited specifications can exhaust this separate preparation budget below
the width ceiling. These are implementation policies, not language constraints
or process-memory guarantees. Layered work policy and release acceptance remain
tracked in [#2300](https://github.com/elong0527/yamaa/issues/2300) and
[#2288](https://github.com/elong0527/yamaa/issues/2288).

## Source installation

The standalone source archive includes the R package and shared Rust workspace.
Source assets are distributed through
[GitHub releases](https://github.com/elong0527/yamaa/releases).

After downloading the tarball from the selected release:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
```

Supported source-build targets are R 4.6.1 on Linux x86_64, macOS arm64 and Windows
x64. Install Rust 1.90.0/Cargo and the platform's C compiler and GNU make first.
Windows builds require Rtools45 and the `x86_64-pc-windows-gnu` Rust target.
The archive contains the
complete Rust workspace and the unchanged canonical Cargo lock. Compilation uses
`cargo build --locked`; no checkout, Python runtime or new dependency resolution
is needed during R installation. Cargo downloads the already locked dependencies
when they are absent from its cache.

For a development build, stage this repository
template before `R CMD build`.
Direct `devtools::install_github(..., subdir = "R/yamaa")` installation omits the
shared Rust sources and is unsupported.

For the check and grammar commands below, install the declared test dependencies,
including jsonlite 2.0.0, in the R library selected for these tests. Run the commands
from the repository root:

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
platforms. The [Rust workspace guide](../../rust/README.md) describes the
crate boundaries and development checks.
