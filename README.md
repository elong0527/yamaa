# yamaa <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)
[![SDTMIG: 3.3+](https://img.shields.io/badge/SDTMIG-3.3%2B-blue)](https://elong0527.github.io/yamaa/articles/supported-standards/#clinical-guides "Minimum supported SDTMIG: 3.3; see the qualified version list and generation/export coverage")
[![ADaMIG: 1.1+](https://img.shields.io/badge/ADaMIG-1.1%2B-blue)](https://elong0527.github.io/yamaa/articles/supported-standards/#clinical-guides "Minimum supported ADaMIG: 1.1; see the qualified version list and generation/export coverage")
[![Define-XML 2.0.0+](https://img.shields.io/badge/Define--XML-2.0.0%2B-blue)](https://www.cdisc.org/standards/data-exchange/define-xml)

[![PyPI version](https://img.shields.io/pypi/v/yamaa)](https://pypi.org/project/yamaa/)
[![rustc](https://img.shields.io/badge/rustc-1.82%2B-orange?logo=rust)](https://www.rust-lang.org)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Release checks](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&label=release%20checks)](https://github.com/elong0527/yamaa/actions/workflows/native.yml "Builds and tests Python and R release packages on Linux, macOS, and Windows, and runs CRAN's incoming checks on the R source package")
[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=workflow_dispatch)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml?query=branch%3Amain+event%3Aworkflow_dispatch)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://codespaces.new/elong0527/yamaa)

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

### Python

Install yamaa from [PyPI](https://pypi.org/project/yamaa/). A release
candidate, when there is one, is a pre-release there, so name it and add
`--pre`:

```sh
python -m pip install yamaa                     # the newest release
python -m pip install --pre yamaa==<candidate>  # a release candidate
```

### R

Install yamaa from the assets of its
[GitHub release](https://github.com/elong0527/yamaa/releases/latest): a binary
package for Windows or macOS and your R series, or the source package. The
[R package page](R/yamaa/README.md#installation) gives every command.
`install.packages("yamaa")` installs the older version that CRAN serves.

```r
install.packages("yamaa_<version>.tar.gz", repos = NULL, type = "source")
```

```r
library(yamaa)
spec <- yamaa_spec("spec.yaml")
spec$check()$issues    # check the specification without reading data
run <- spec$run()      # build the dataset in memory
run$output
run$save()             # write adsl.csv
```

## Where to go next

- New here? [Introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the model in five minutes.
- Running specifications? [R, Python and command-line API](https://elong0527.github.io/yamaa/articles/api/) shows every call in both languages and every command.
- Building on yamaa? The [stability contract](https://github.com/elong0527/yamaa/blob/main/rust/interface/README.md#stability-contract) lists what every 1.x release keeps from 1.0.0.
- Coming from Excel specs? [Excel to yamaa](https://elong0527.github.io/yamaa/articles/excel/) translates what you already know.
- Looking for worked examples? [Benchmarks](https://elong0527.github.io/yamaa/benchmark/) illustrate the ideas.
- Developing the engine? [Contributing](CONTRIBUTING.md) describes the shared architecture, checks and releases; each [release](https://github.com/elong0527/yamaa/releases) lists its supported platforms and changes.

## License

This project is licensed under the terms of the MIT license. The Python and R
packages and the command-line binaries also include third-party code under its
own licenses; each package carries those notices in `LICENSE-THIRD-PARTY.txt`,
with an inventory of its components in `COMPONENTS.tsv`.
