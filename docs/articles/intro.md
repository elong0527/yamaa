# Motivation

Two AI agents use the same specification to derive ADaM data.
Both report completion, but their outputs differ.
The specification leaves decisions for the agents to make during implementation.
A human reviewer must find the gap, settle the rule, and rerun the derivation.

Review takes longer, and confidence in the result falls.

# Introduction

yamaa is a domain-specific language (DSL) for clinical trial data
standardization. A yamaa specification transforms ODM XML data, extracted from
an EDC system, into SDTM and ADaM datasets following CDISC standards, and on
to define.xml. yamaa's rules fix what every item means, so the same
specification with the same inputs always produces the same dataset, in the R
engine and in the Python engine alike.

The language is designed for AI-agent and human collaboration: people and
agents build the specification together at the planning stage, so nothing is
left for the coding stage to guess.

## Why specifications disagree

An Excel specification has three layers, but only two of them are written down:

| Layer | What it answers | Where an Excel spec keeps it |
|---|---|---|
| **Structure** | What exists -- which datasets, which variables, which types | Dataset sheet, Variable sheet |
| **Algorithm** | What to compute | The free-text derivation column |
| **Semantics** | What that computation means **when the data does not cooperate** | **Nowhere.** It lives in each programmer's experience |

yamaa writes each layer into a different kind of file:

| Layer of yamaa | File |
|---|---|
| **Structure** | `yaml/schema*.yaml` -- what a specification may contain |
| **Algorithm** | your `spec.yaml` -- one specification produces one dataset |
| **Semantics** | `rules/` -- normative contracts for every written item |
| **Worked proof** | `benchmarks/` -- runnable specifications with byte-exact expected outputs |

In one sentence: **an Excel spec is written to be understood; a yamaa spec is
written to be executed the same way twice.**

## One execution

> **A yamaa specification has exactly one execution. Where it would have two,
> yamaa fails instead of choosing.**

Think of execution as a function:

> `output_dataset = derive(input_datasets, spec)`

The contract covers two things: **spec**, the closed vocabulary saying what
you can write and nothing more, and **derive**, the semantics saying what
each written item means. See [Principles](principles.md) for why this moves
agreement to the planning stage.

## The four components

| Component | Purpose | Start here |
|---|---|---|
| Schema | Declares what a specification may contain; anything it does not declare is rejected before execution. | [Schema introduction](schema-intro.md) |
| Rules | Fix the meaning of every written item, so the R and Python engines cannot read the same spec in two ways. | [Rules](rules.md) |
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | [R and Python API](api.md) |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs. | [Benchmark](../benchmark/index.md) |

## What a specification looks like

One specification produces one dataset. yamaa first constructs the output
rows, then derives columns onto those rows. **Column derivation never changes
the row count.** See [Derivation](derive.md) for the full keys / rows /
columns walkthrough.

Derivations use only registered verbs -- no free-text derivation column.
Every verb declares what happens when the data does not cooperate:

```yaml
derivation:
  mapping:
    source: DM.SEX
    dict: {M: M, F: F}
    missing: U      # "if not collected, set to U"
    unmapped: U     # "if not in codelist, set to U"
```

**Omit a handler and its condition stops the run.** "Not collected" and
"collected but unrecognised" are two conditions, answered by `missing` and
`unmapped`. Write `null` as the answer to leave the value missing instead.

## Programs in a study submission

The sponsor chooses the program format and its location in the submission
package with its submission team. The program can be the YAML specification
with its R or Python driver, or a readable R or Python implementation,
including code produced with AI assistance. The sponsor prepares and reviews
any separate implementation as part of its own development process.

Retain the exact inputs, specification and inherited files, project file,
codelists and other referenced resources, driver, project functions, and
environment declarations used for the build. Record the yamaa version and the host and
dependency versions. The sponsor determines which of these accompany the
submitted programs alongside the datasets, define.xml and reviewer's guide.

Verify a separate implementation independently against the yamaa results.
Match records by their declared keys and compare every output cell, including
missing values and types; state any numeric tolerance in the comparison
record. Retain the comparison report and resolve discrepancies before using
the implementation as the submitted program. Use a separately implemented
derivation for this verification: the R and Python interfaces share an engine.

The reviewer's guide can explain the approach with wording such as:

> The YAML specifications state the dataset structure and each variable's
> derivation. The accompanying driver runs those specifications with the
> recorded yamaa version and environment. The program inventory identifies
> the source files used for each dataset and the retained verification
> evidence.

Adapt this text to the chosen program format; for a separate R or Python
implementation, identify its cell-level comparison evidence. The derivation
descriptions and computational methods in define.xml should agree with the
specification, and the reviewer's guide should explain where to find each.
This guide documents no filed-study precedent or agency acceptance; the
sponsor owns the submission decision and supporting evidence.

## Where to go next

- Why one execution: [Principles](principles.md).
- How rows and columns work: [Derivation](derive.md).
- Coming from Excel specs: [Excel to yamaa](excel.md).
- How to read a runnable example: [Benchmark](../benchmark/index.md).
