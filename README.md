# yamaa <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Release checks](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&label=release%20checks)](https://github.com/elong0527/yamaa/actions/workflows/native.yml "Builds and tests Python and R release packages on Linux, macOS, and Windows, and runs CRAN's incoming checks on the R source package")
[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=workflow_dispatch)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml?query=branch%3Amain+event%3Aworkflow_dispatch)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![PyPI version](https://img.shields.io/pypi/v/yamaa)](https://pypi.org/project/yamaa/)
[![rustc](https://img.shields.io/badge/rustc-1.82%2B-orange?logo=rust)](https://www.rust-lang.org)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

yamaa is a domain-specific language (DSL) for clinical trial data standardization.
A yamaa specification transforms ODM XML data, extracted from an EDC system,
into SDTM and ADaM datasets following CDISC standards. The engine can also
generate define.xml.
yamaa's rules fix what every item means, so the same specification with the same
inputs always produces the same datasets, whether the R package, the Python
package or the `yamaa` command line runs it.

The one principle behind everything:

> A yamaa specification has exactly one execution. Where it would have two,
> yamaa fails instead of choosing.

That is what makes it safe to hand a specification to an AI agent: every
question gets answered while it is still cheap to answer, at the planning
stage. Nothing is left for the coding stage to guess.

The language is written to be read and revised by people and AI agents
together, keeping derivations reviewable, version-controlled, and consistent
across implementations. The yamaa project has four components: schema, rules,
engine, and benchmark.

| Component | Purpose | Links |
| --- | --- | --- |
| Schema | Declares the vocabulary of the specification. | [Repository](https://github.com/elong0527/yamaa/tree/main/yaml) / [Introduction](https://elong0527.github.io/yamaa/articles/schema-intro/) |
| Rules | Defines the meaning of each specification. | [Repository](https://github.com/elong0527/yamaa/tree/main/rules) / [Rules](https://elong0527.github.io/yamaa/articles/rules/) |
| Engine | Executes specifications in Rust for Python, R and the command line. | [Rust](https://github.com/elong0527/yamaa/tree/main/rust) / [Python](https://github.com/elong0527/yamaa/tree/main/python) / [R](https://github.com/elong0527/yamaa/tree/main/R/yamaa) |
| Benchmark | Provides runnable specification examples. | [Repository](https://github.com/elong0527/yamaa/tree/main/benchmarks) / [Dashboards](https://elong0527.github.io/yamaa/benchmark/) |

## From ODM XML to SDTM and ADaM

The engine reads ODM XML extracted from the EDC system and transforms it into
one long-form clinical-item table. Each row represents one recorded item and
retains the item's study, event, form, and item-group context. A yamaa
specification, run from either package, reads the transformed data
([example](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-basic.html))
and derives SDTM and ADaM datasets.

## Inheritance

![yamaa design: inherited templates become study specifications that drive validated SDTM, ADaM](https://raw.githubusercontent.com/elong0527/yamaa/main/docs/diagrams/design.svg)

Reusable yamaa specification templates flow from the organization level through
the compound and study levels. Approved study specifications then drive
deterministic, validated builds while preserving metadata lineage.

## Example

A complete specification in 33 lines:

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]        # one row per subject
input:
  # CSV fields are text, and cut needs a numeric source.
  DM: {path: input/dm.csv, types: {AGE: int}}

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGEGR1]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: AGEGR1
    type: str
    label: Pooled Age Group 1
    derivation:
      cut:
        source: DM.AGE
        breaks: [18, 65]
        labels: ['<18', '18-64', '>=65']
        missing: UNKNOWN
```

Read it top to bottom. The first line names the oldest language version the
file needs, the second says the file is a specification, and `model` names
its CDISC model. Then come two keys, one input that drives the rows, and three
columns. A derivation is an expression or a verb from a closed registry, such
as `cut`, `mapping` or `case` (see [Schema introduction](https://elong0527.github.io/yamaa/articles/schema-intro/)).
In an expression, a bare name such as `DM.STUDYID` reads a column, text is
quoted, as in `derivation: "'Y'"`, and YAML `null` is the missing value. A
verb's value fields, such as `labels` and `missing` above, hold only
constants, so `UNKNOWN` there is text.

## Installation

Python engine:

Install yamaa from [PyPI](https://pypi.org/project/yamaa/). A release
candidate, when there is one, is a pre-release there, so name it and add
`--pre`:

```sh
python -m pip install yamaa                     # the newest release
python -m pip install --pre yamaa==<candidate>  # a release candidate
```

The wheels and a standalone Python source archive are also assets of each
[GitHub release](https://github.com/elong0527/yamaa/releases). See the
[Python package README](https://github.com/elong0527/yamaa/blob/main/python/README.md)
for source prerequisites, development setup and supported platforms.

R engine:

Each [GitHub release](https://github.com/elong0527/yamaa/releases) carries the
R source package and, from 0.6.0 on, binary packages
for Windows x64 and macOS arm64 built for R-release and R-oldrel, today R 4.6
and R 4.5. A binary package installs without Rust, Rtools or a network, and
works only with the R series that ends its file name. Download the package for
your platform and R from the selected release, and install it:

```r
# Windows x64
install.packages("yamaa_<version>-4.6.zip", repos = NULL)  # R 4.6
install.packages("yamaa_<version>-4.5.zip", repos = NULL)  # R 4.5
# macOS arm64
install.packages("yamaa_<version>-4.6.tgz", repos = NULL)  # R 4.6
install.packages("yamaa_<version>-4.5.tgz", repos = NULL)  # R 4.5
# Linux, or any other R series: the source package
install.packages("yamaa_<version>.tar.gz", repos = NULL, type = "source")
```

Once CRAN accepts 0.6.2, `install.packages("yamaa")` installs it from there
too. Then load the package and run the example above:

```r
library(yamaa)
spec <- yamaa_spec("spec.yaml")
spec$check()$issues    # check the specification without reading data
run <- spec$run()      # build the dataset in memory
run$output
run$save()             # write adsl.csv
```

Source installation requires R 4.2.0 or later, Rust 1.82 or later with Cargo, and platform build tools; it needs no network.
To build the package from a checkout, follow
[the R package guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md).
The **Release checks** badge shows the latest release package checks on `main`,
which run weekly and before each release: Python and R installation, benchmark
conformance, and numerical consistency across Linux, macOS, and Windows, and
CRAN's incoming checks of the R source package (`R CMD check --as-cran` with
R-devel, R-release and R 4.2.0 and the minimum Rust, once more with R-devel on
Linux and none of the suggested packages, and in a CRAN-like Fedora
container). Release assets come from that workflow.

On a machine without internet access, install from an internal mirror or from
files carried in, as
[Installing in a restricted environment](https://elong0527.github.io/yamaa/articles/restricted-environments/)
describes.

Packages from 0.6.0 on carry a self-test, which earlier packages do not.
After installing one, run `yamaa.selftest()` in Python or
`yamaa_selftest()` in R. It runs the cases that the package carries,
benchmarks with independently authored expected files, through the installed
engine and compares each result with its expected files, writing no file and
using no network. Its printed report says whether every case passed, names
any adapter that no case covers, and gives the engine version, the language
versions, the host and its version, and the platform: keep it as the
installation and operational qualification evidence of that installation.

Command line:

From 0.6.0, each GitHub release also carries the `yamaa` command line as a
standalone binary that needs no Python, R or Rust:
`yamaa-<version>-<target>.zip`, where the target is
`x86_64-unknown-linux-musl` or `aarch64-unknown-linux-musl` for Linux,
`x86_64-pc-windows-msvc` for Windows x64 and `aarch64-apple-darwin` for macOS
arm64 (macOS 11 or later). There is no installer: unzip the archive and run
`yamaa` (`yamaa.exe` on Windows) in the directory that it holds, beside the
license notices and the component inventory, or put that directory on your
`PATH`. The Linux binaries are static, so they run on any Linux of their
architecture. `yamaa --version` names the engine, and `yamaa selftest` runs
the self-test above.
The API page's
[command line](https://elong0527.github.io/yamaa/articles/api/#command-line)
section lists every command and flag, the exit statuses and the JSON output.

The binaries are not signed. On macOS, a binary that a browser downloaded
carries the quarantine attribute, and Gatekeeper refuses to run it, saying
that Apple could not verify it. Clear the attribute once, in the archive's
directory:

```sh
xattr -d com.apple.quarantine yamaa
```

On Windows, SmartScreen may stop a downloaded `yamaa.exe` with "Windows
protected your PC": choose **More info**, then **Run anyway**, or remove the
download mark first with `Unblock-File` in PowerShell.

## Where to go next

- New here? [Introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the model in five minutes.
- Running specifications? [R, Python and command-line API](https://elong0527.github.io/yamaa/articles/api/) shows every call in both languages and every command.
- Building on yamaa? The [stability contract](https://elong0527.github.io/yamaa/release/#stability-contract) lists what every 1.x release keeps from 1.0.0.
- Coming from Excel specs? [Excel to yamaa](https://elong0527.github.io/yamaa/articles/excel/) translates what you already know.
- Looking for worked examples? [Benchmarks](https://elong0527.github.io/yamaa/benchmark/) illustrate the ideas.
- Developing the engine? [Contributing](CONTRIBUTING.md) describes the shared architecture and checks; [release guidance](docs/release.md) records supported platforms and migration notes.

## License

This project is licensed under the terms of the MIT license. The Python and R
packages and the command-line binaries also include third-party code under its
own licenses; each package carries those notices in `LICENSE-THIRD-PARTY.txt`
and the other files listed in the
[release guidance](docs/release.md#source-package-contents), with an inventory
of its components in `COMPONENTS.tsv`.
