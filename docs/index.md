# yamaa <img src="assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />

[![Python tests](https://github.com/elong0527/yamaa/actions/workflows/python.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/python.yml)
[![YAML validation](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/deploy-docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

yamaa is a domain-specific language (DSL) for clinical trial data
standardization. A yamaa specification transforms ODM XML data, extracted from
an EDC system, into SDTM and ADaM datasets following CDISC standards, and on
to define.xml. yamaa's rules fix what every item means, so the same
specification with the same inputs always produces the same dataset, in the R
engine and in the Python engine alike.

The one principle behind everything:

> **A yamaa specification has exactly one execution. Where it would have two,
> yamaa fails instead of choosing.**

That is what makes it safe to hand a specification to an AI agent: every
question gets answered while it is still cheap to answer, at the planning
stage. Nothing is left for the coding stage to guess.

## The four components

| Component | Purpose | Documentation |
|---|---|---|
| Schema | Declares the vocabulary of the language: what a specification may contain. Anything the schema does not declare is rejected before execution. | [Schema introduction](articles/schema-intro.md), [Schema reference](articles/schema.md) |
| Rules | Fix the meaning of every written item, so the R and Python engines execute the same specification in exactly one way. | [Rules](articles/rules.md) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [Python engine](https://github.com/elong0527/yamaa/tree/main/python), [R engine](https://github.com/elong0527/yamaa/tree/main/R/cdiscbuilder) |
| Benchmark | 218 runnable specifications with input data and byte-exact expected outputs. | [Reading the examples](articles/benchmark.md), [Benchmark](benchmark/index.md) |

## A complete specification in 20 lines

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
    derivation: {source: DM.STUDYID}
  - name: USUBJID
    type: str
    derivation: {source: DM.USUBJID}
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
closed registry -- see [Schema introduction](articles/schema-intro.md).

## Where the ODM data comes in

The engine reads the ODM XML extracted from the EDC system -- a plain file or a
TAR archive -- and projects it into one long-form clinical-item table: one row
per recorded item, keeping that item's study, event, form and item-group
context. A specification reads that projection, addresses an item by a
predicate over `ItemOID`, and derives SDTM and ADaM columns onto the rows it
constructs. The benchmarks ship the projection directly as a small `odm.csv`,
so each example stays reviewable by eye.

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

- New here? [Introduction](articles/intro.md) explains the model in five minutes.
- Coming from Excel specs? [Excel to yamaa](articles/excel.md) translates what you already know.
- Want worked examples? [Reading the examples](articles/benchmark.md) shows how to read the 218 benchmarks.

## License

This project is licensed under the terms of the MIT license.
