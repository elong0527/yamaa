# yamaa <img src="https://raw.githubusercontent.com/elong0527/yamaa/main/docs/assets/logo.jpeg" align="right" width="120" alt="yamaa logo" />


[![CI](https://github.com/elong0527/yamaa/actions/workflows/ci.yml/badge.svg?branch=main&event=push)](https://github.com/elong0527/yamaa/actions/workflows/ci.yml)
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
