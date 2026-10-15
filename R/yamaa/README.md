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
issue context is JSON text. Capacity refusals retain the shared condition and
authored paths; [release guidance](../../docs/release.md#resource-refusals)
describes their context. `yamaa_check` reads no study data or project code.
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

The shared hosts use the same [resource policies](../../docs/release.md#resource-limits).
That page owns all capacity limits and refusal details. These are implementation
policies, not language constraints or process-memory guarantees.

## Source installation

The standalone source archive includes the R package and shared Rust workspace.
Source assets are distributed through
[GitHub releases](https://github.com/elong0527/yamaa/releases).

After downloading the tarball from the selected release:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
```

The release asset is the unchanged source archive tested on Linux. Native
qualification also builds, installs and checks each host's own source archive
on macOS and Windows. Checkout builds are development candidates; release
downloads are selected by the complete native qualification workflow.

Supported source-build targets are R 4.6.1 on Linux x86_64, macOS arm64 and Windows
x64. Install Rust 1.90.0/Cargo and the platform's C compiler and GNU make first.
Windows builds require Rtools45 and the `x86_64-pc-windows-gnu` Rust target.
The archive contains the
complete Rust workspace and the unchanged canonical Cargo lock. Compilation uses
`cargo build --locked`; no checkout, Python runtime or new dependency resolution
is needed during R installation. Cargo downloads the already locked dependencies
when they are absent from its cache.

For a repository development build, use the setup and package-check
targets documented by `make help` and the [contributor guide](../../CONTRIBUTING.md).
Direct installation of the `R/yamaa/` template omits the shared Rust sources;
install a staged source archive instead. Staging refuses an existing destination.

The source archive carries unchanged authored grammar documents and independent
installed test inputs. Native qualification installs packages outside the
checkout and requires `Status: OK` from strict `R CMD check` on each supported
platform. [Installed package checks](../../rust/qualification/README.md) describe
that separate qualification route.
