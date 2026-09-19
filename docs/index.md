---
title: YAMAA documentation
---

# YAMAA <img src="assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

[![Python tests](https://github.com/elong0527/yamaa/actions/workflows/python.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/python.yml)
[![YAML validation](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml/badge.svg)](https://github.com/elong0527/yamaa/actions/workflows/yaml-validation.yml)
[![Docs](https://github.com/elong0527/yamaa/actions/workflows/deploy-docs.yml/badge.svg)](https://elong0527.github.io/yamaa/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://github.com/elong0527/yamaa/blob/main/LICENSE)

Language-neutral YAML specification for reproducible clinical trial data pipelines.

Designed for AI-agent and human collaboration on clinical data standardization, following our [core principles](articles/principles.md). Engines in Python and R run YAMAA specifications, and minimal visible benchmarks demonstrate them in real cases.

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

## Agentic exploration

The fastest way to explore YAMAA is with an AI agent. For example, ask an agent to:

- assess whether one ADaM dataset derivation can be migrated to a YAMAA specification, and run it with a language engine;
- explain the design of YAMAA from https://github.com/elong0527/yamaa.

## Documentation

- [Principles](articles/principles.md) -- the short answer to "what is YAMAA for": one execution.
- [Why YAMAA](articles/why-yamaa.md) -- the argument before the syntax.
- [Excel to YAMAA](articles/excel-to-yamaa.md) -- translating specifications you already have.
- [Schema concepts](articles/schema-concepts.md) -- the language itself.
- [Benchmark](benchmark/index.md) -- every benchmark, input against output.

## License

This project is licensed under the terms of the MIT license.
