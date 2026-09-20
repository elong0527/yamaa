# Introduction

YAMAA is a domain-specific language (DSL) for clinical trial data
standardization. A YAMAA specification transforms ODM XML data, extracted from
an EDC system, into SDTM and ADaM datasets following CDISC standards, and on
to define.xml. YAMAA's rules fix what every item means, so the same
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

Two programmers read the same derivation column. Both are certain they
understood it. Their programs disagree -- not from misreading, but because
the specification never reached the point they disagreed on: what missing
means, what a duplicate means, how dates compare.

YAMAA writes each layer into a different kind of file:

| Layer of YAMAA | File |
|---|---|
| **Structure** | `yaml/schema*.yaml` -- what a specification may contain |
| **Algorithm** | your `spec.yaml` -- one specification produces one dataset |
| **Semantics** | `yaml/rules/` -- normative contracts for every written item |
| **Worked proof** | `benchmark/` -- 214 runnable specifications with byte-exact expected outputs |

In one sentence: **an Excel spec is written to be understood; a YAMAA spec is
written to be executed the same way twice.**

## One execution

> **A YAMAA specification has exactly one execution. Where it would have two,
> YAMAA fails instead of choosing.**

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
| Engine | Runs specifications in Python and R; the same specification with the same inputs produces the same output dataset. | Python and R engines under Engine in the nav |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs. | [Reading the examples](benchmark.md) |

## What a specification looks like

One specification produces one dataset. YAMAA first constructs the output
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

**Omit a handler and its condition is fatal.** "Not collected" and
"collected but unrecognised" are always two questions, and both must be
answered in writing.

## Where to go next

- Why one execution: [Principles](principles.md).
- How rows and columns work: [Derivation](derive.md).
- Coming from Excel specs: [Excel to YAMAA](excel.md).
- How to read a runnable example: [Reading the examples](benchmark.md).
