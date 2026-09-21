# yamaa <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![Python tests](https://github.com/elong0527/yamaa/actions/workflows/python.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/python.yml)
[![YAML validation](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/deploy-docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

yamaa is a domain-specific language (DSL) for clinical trial data standardization.
A yamaa specification transforms ODM XML data, extracted from an EDC system,
into SDTM and ADaM datasets following CDISC standards. The engine can also
generate define.xml.
yamaa's rules fix what every item means, so the same specification with the same
inputs always produce the same datasets in both the R and Python engines.

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
| Rules | Defines the meaning of each specification. | [Repository](https://github.com/elong0527/yamaa/tree/main/yaml/rules) / [Rules](https://elong0527.github.io/yamaa/articles/rules/) |
| Engine | Runs specifications in Python and R according to the rules. | [Python](https://github.com/elong0527/yamaa/tree/main/python) / [R](https://github.com/elong0527/yamaa/tree/main/R/cdiscbuilder) |
| Benchmark | Provides runnable specification examples. | [Repository](https://github.com/elong0527/yamaa/tree/main/benchmark) / [Dashboards](https://elong0527.github.io/yamaa/benchmark/) |

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

A complete specification in 20 lines:

```yaml
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]        # one row per subject
input:
  DM: input/dm.csv
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

```
pip install git+https://github.com/elong0527/yamaa.git#subdirectory=python
```

R engine:

```r
# install.packages("devtools")
devtools::install_github("elong0527/yamaa", subdir = "R/cdiscbuilder")
```

## Where to go next

- New here? [Introduction](https://elong0527.github.io/yamaa/articles/intro/) explains the model in five minutes.
- Coming from Excel specs? [Excel to yamaa](https://elong0527.github.io/yamaa/articles/excel/) translates what you already know.
- Looking for worked examples? [Benchmarks](https://elong0527.github.io/yamaa/benchmark/) illustrate the ideas.

## License

This project is licensed under the terms of the MIT license.
