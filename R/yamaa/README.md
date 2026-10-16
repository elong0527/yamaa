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

Pass `resolver=` to `yamaa_domain` or `yamaa_check` for host-owned metadata,
inputs and Save destinations. Default calls use native local files. The shared
contract is described in [Host resources](../../python/README.md#host-resources).

```r
resources <- yamaa_memory_resolver(list(
  "spec.yaml" = charToRaw(specification_text),
  "raw.csv" = charToRaw("ID\n7\n")
))
result <- yamaa_domain("spec.yaml", resolver = resources)
result$resource_provenance
result$save()
resources$backend$outputs
```

`yamaa_synthetic_remote_resolver` provides the same deterministic behavior with
synthetic object-store identities. `yamaa_resource_resolver(callback)` wraps a
callback that takes a typed request and returns `yamaa_resource_result`.
Use `yamaa_resource_failure` for explicitly classified failures. Inspect and
Target do not read study bytes; Capture returns raw bytes or an immutable
Arrow-compatible frame. Stage, Commit and Abort publish held bytes with cleanup
and preserve the completed publication prefix. Conditions and interrupts retain
their original R objects through cleanup.

`resources$backend$put("raw.csv", NULL, frame = frame)` captures immutable
nanoarrow arrays and schema once, then supplies a fresh C Stream on every
capture, including repeated aliases and later builds. Generic callbacks returning
a stream must supply a fresh stream on every call. Supported fields are UTF-8,
signed 64-bit integers, doubles, Date32 and whole-second timestamps without a
timezone. No IPC or temporary input files are used for frames. References and
held-content comparison establish snapshot freshness; no content hashing is used.
Credentials remain private to the callback, independently from retained
`resource_provenance`. Returned URI references do not change authored path rules.

`yamaa_domain` computes without publishing files. Explicit Save publishes retained
producer bytes before consumers and reuses those bytes on repeated calls. A failed
build raises `yamaa_domain_error` on Save. Owned result attributes are read only;
issue context is JSON text. Capacity refusals retain the shared condition and
authored paths; [release guidance](../../docs/release.md#resource-refusals)
describes their context. `yamaa_check` reads no study data or project code.
Resource paths resolve from their original declaring YAML layer by default.
Set `relative_to: entry` on an input dataset or `output` declaration to select
the study entry's directory under the
[local-path policy](../../rules/storage/resources.md). Input settings cover
both data paths and producer `schema` links; output settings cover the primary
artifact and both logs. A producer uses its own linked entry file, which may
inherit shared code. `result$save()` publishes the held bytes to those selected
destinations.

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

With nanoarrow 0.8.0 or newer installed, `result$output_arrow`,
`result$verification_log_arrow` and `result$warning_log_arrow` return standard
`nanoarrow_array_stream` objects. Every access creates an independent stream
cursor sharing immutable Arrow buffers. Nanoarrow owns each stream header and
calls its release callback on explicit release or garbage collection. Imported
arrays remain valid after releasing the stream or build result. These views
preserve signed i64 extrema, NUL UTF-8, nulls and date/datetime epoch values.
The ordinary data-frame views explicitly copy into the established exact R
representations above; conversion to R character or numeric preserves values
or raises an error. Generic consumers must choose equally exact conversions.
Nanoarrow 0.7.0 uses an R-internals API removed in the supported R 4.6.1, so the
bridge floor is 0.8.0. See the [ownership contract](../../rust/README.md#arrow-host-handoff).

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
