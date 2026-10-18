# yamaa <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
[![Regular build](https://img.shields.io/github/actions/workflow/status/elong0527/yamaa/native.yml?branch=main&event=schedule&label=regular%20build)](https://github.com/elong0527/yamaa/actions/workflows/native.yml)
[![R CMD check](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml/badge.svg?branch=main&event=schedule)](https://github.com/elong0527/yamaa/actions/workflows/R-CMD-check.yaml)
[![Coverage](https://codecov.io/gh/elong0527/yamaa/graph/badge.svg)](https://codecov.io/gh/elong0527/yamaa)
[![PyPI version](https://img.shields.io/pypi/v/yamaa)](https://pypi.org/project/yamaa/)
[![rustc](https://img.shields.io/badge/rustc-1.90%2B-orange?logo=rust)](https://www.rust-lang.org)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

yamaa is a domain-specific language (DSL) for clinical trial data standardization.
A yamaa specification transforms ODM XML data, extracted from an EDC system,
into SDTM and ADaM datasets following CDISC standards. The engine can also
generate define.xml.
yamaa's rules fix what every item means, so the same specification with the same
inputs always produces the same datasets through both the R and Python packages.

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
| Engine | Executes specifications in Rust for Python and R. | [Rust](https://github.com/elong0527/yamaa/tree/main/rust) / [Python](https://github.com/elong0527/yamaa/tree/main/python) / [R](https://github.com/elong0527/yamaa/tree/main/R/yamaa) |
| Benchmark | Provides runnable specification examples. | [Repository](https://github.com/elong0527/yamaa/tree/main/benchmarks) / [Dashboards](https://elong0527.github.io/yamaa/benchmark/) |

## From ODM XML to SDTM and ADaM

The engine reads ODM XML extracted from the EDC system and transforms it into
one long-form clinical-item table. Each row represents one recorded item and
retains the item's study, event, form, and item-group context. A yamaa
specification reads the transformed data
([example](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-basic.html))
and derives SDTM and ADaM datasets.

## Inheritance

![yamaa design: inherited templates become study specifications that drive validated SDTM, ADaM](https://raw.githubusercontent.com/elong0527/yamaa/main/docs/diagrams/design.svg)

Reusable yamaa specification templates flow from the organization level through
the compound and study levels. Approved study specifications then drive
deterministic, validated builds while preserving metadata lineage.

## Example

A complete specification in 24 lines:

```yaml
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]        # one row per subject
input:
  DM: {path: input/dm.csv, types: {AGE: int}}   # CSV fields are text; cut needs a numeric source
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGEGR1]

columns:
  - name: STUDYID
    type: str
    derivation: DM.STUDYID
  - name: USUBJID
    type: str
    derivation: DM.USUBJID
  - name: AGEGR1
    type: str
    derivation:
      cut:
        source: DM.AGE
        breaks: [18, 65]
        labels: ['<18', '18-64', '>=65']
        missing: UNKNOWN
```

Read it top to bottom: one input, one driver, two keys, three columns. The
derivation verbs (`source`, `cut`, `mapping`, `compute`, ...) come from a
closed registry (see [Schema introduction](https://elong0527.github.io/yamaa/articles/schema-intro/)).

## Installation

Python engine:

Download the wheel for your platform from the selected
[GitHub release](https://github.com/elong0527/yamaa/releases), then install it:

```sh
python -m pip install ./yamaa-0.4.0-<wheel-tags>.whl
```

The release also provides a standalone Python source archive. See the
[Python package guide](https://github.com/elong0527/yamaa/blob/main/python/README.md)
for supported platforms, source prerequisites and checkout development builds.

R engine:

R is distributed as a standalone source tarball attached to its
[GitHub release](https://github.com/elong0527/yamaa/releases). After downloading
`yamaa_0.4.0.tar.gz` from the selected qualified release:

```r
install.packages("yamaa_0.4.0.tar.gz", repos = NULL, type = "source")
library(yamaa)
result <- yamaa_domain("spec.yaml")
result$output
yamaa_check("spec.yaml")$issues
result$save()
```

Source installation requires R 4.6.1, Rust 1.90 or later with Cargo, and platform build tools.
To build a checkout candidate, follow
[the R package guide](https://github.com/elong0527/yamaa/blob/main/R/yamaa/README.md).
Release assets come from the complete native qualification workflow. CRAN and r-universe
publication are later distribution work.

## Where to go next

- New here? [Introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the model in five minutes.
- Coming from Excel specs? [Excel to yamaa](https://elong0527.github.io/yamaa/articles/excel/) translates what you already know.
- Looking for worked examples? [Benchmarks](https://elong0527.github.io/yamaa/benchmark/) illustrate the ideas.
- Developing the engine? [Contributing](CONTRIBUTING.md) describes the shared architecture and checks; [release guidance](docs/release.md) records supported platforms and migration notes.

## License

This project is licensed under the terms of the MIT license.
