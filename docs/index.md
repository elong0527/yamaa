# YAMAA <img src="assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

[![Python tests](https://github.com/elong0527/yamaa/actions/workflows/python.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/python.yml)
[![YAML validation](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/deploy-docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

YAMAA is a language-neutral YAML specification for reproducible clinical
trial data pipelines. You write what each dataset contains; the R and Python
engines execute it the same way, every time, from EDC extraction through
SDTM and ADaM to define.xml.

It is designed for AI-agent and human collaboration: people and agents build
the specification together at the planning stage, so nothing is left for the
coding stage to guess.

YAMAA has four components: schema, rules, engine and benchmark. 

| Component | Purpose | Documentation |
|---|---|---|
| Schema | Declares what a specification may contain; anything the schema does not declare is rejected before execution. | [Schema concepts](articles/schema-concepts.md), [Schema reference](reference/schema.md) |
| Rules | Fix the meaning of every written item, so the R and Python engines execute the same specification in exactly one way. | [Rules](reference/rules.md) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [Python engine](https://github.com/elong0527/yamaa/tree/main/python), [R engine](https://github.com/elong0527/yamaa/tree/main/R/cdiscbuilder) |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs | [Benchmark](benchmark/index.md) |

## Agentic exploration

The fastest way to explore YAMAA is with an AI agent. For example, ask an agent to:

- assess whether one ADaM dataset derivation can be migrated to a YAMAA specification, and run it with a language engine;
- explain the design of YAMAA from https://github.com/elong0527/yamaa.

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

## License

This project is licensed under the terms of the MIT license.
