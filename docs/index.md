---
title: YAMAA documentation
---

# YAMAA <img src="assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

[![Python tests](https://github.com/elong0527/yamaa/actions/workflows/python.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/python.yml)
[![YAML validation](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/deploy-docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

YAMAA is a language-neutral YAML specification for reproducible clinical
trial data pipelines. You write what each dataset contains; the R and Python
engines execute it the same way, every time.

The one principle behind everything:

> **A YAMAA specification has exactly one execution. Where it would have two,
> YAMAA fails instead of choosing.**

That is what makes it safe to hand a specification to an AI agent: every
question gets answered while it is still cheap to answer, at the planning
stage. Nothing is left for the coding stage to guess.

## The four components

| Component | Purpose | Documentation |
|---|---|---|
| Schema | Declares what a specification may contain; anything the schema does not declare is rejected before execution. | [Spec anatomy](articles/spec-anatomy.md), [Schema reference](reference/schema.md) |
| Rules | Fix the meaning of every written item, so the R and Python engines execute the same specification in exactly one way. | [Rules](reference/rules.md) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [Python engine](https://github.com/elong0527/yamaa/tree/main/python), [R engine](https://github.com/elong0527/yamaa/tree/main/R/cdiscbuilder) |
| Benchmark | 213 runnable specifications with input data and byte-exact expected outputs. | [Reading the examples](articles/examples.md), [Benchmark](benchmark/index.md) |

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
closed registry -- see [Spec anatomy](articles/spec-anatomy.md).

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

- New here? [Core concepts](articles/concepts.md) explains the model in five minutes.
- Coming from Excel specs? [Excel to YAMAA](articles/excel-to-yamaa.md) translates what you already know.
- Want worked examples? [Reading the examples](articles/examples.md) shows how to read the 213 benchmarks.

## License

This project is licensed under the terms of the MIT license.
