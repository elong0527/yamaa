---
title: Core concepts
---

# Core concepts

## The idea in one table

An Excel specification has three layers, but only two of them are written down:

| Layer | What it answers | Where an Excel spec keeps it | Who can read it |
|---|---|---|---|
| **Structure** | What exists -- which datasets, which variables, which types | Dataset sheet, Variable sheet | People, and machines with effort |
| **Algorithm** | What to compute | The free-text derivation column | People only |
| **Semantics** | What that computation means **when the data does not cooperate** | **Nowhere.** It lives in each programmer's experience | Nobody |

YAMAA writes each layer into a different kind of file:

| Layer of YAMAA | File |
|---|---|
| **Structure** | `yaml/schema*.yaml` -- the header row of the Variable sheet |
| **Algorithm** | your `spec.yaml` -- the body of the Variable sheet, derivation column included |
| **Semantics** | `yaml/rules/` -- the conventions that were never written down, as normative semantic contracts |
| **Worked benchmarks** | `benchmark/` -- 213 runnable specifications with byte-exact expected outputs |

In one sentence: **an Excel spec is written to be understood; a YAMAA spec is
written to be executed the same way twice.**

## One execution

> `output_dataset = derive(input_datasets, spec)`

Two programmers read the same specification. Both are certain they understood
it. Their programs disagree. That disagreement is almost never a misreading --
it is that the specification never reached the point they disagreed on:

- **"What missing means."** A spec says `AVAL = LBSTRESN`. The source cell might
  be empty, `NA`, `.`, or `NOT DONE`. Which of those is missing? YAMAA
  separates *the variable does not exist* (usually a broken extract) from *the
  row is empty* (ordinary clinical reality).
- **"What a duplicate means."** `TRTSDT: Predecessor ADSL.TRTSDT, merge by
  USUBJID` -- what if a subject has two ADSL records? A SAS merge silently
  keeps the last one or makes a cartesian product, and nothing leaves a mark.
  YAMAA fails on multiple matches unless you declare `multiple_matches`.
- **"How dates compare."** `ASTDT >= TRTSDT` works as a string comparison
  until one side is `"2024-1-10"` or a partial date `"2024-01"`. YAMAA owns
  temporal values completely: which text becomes a date, how two of them
  order, and what canonical text they are written back as.

The design answers each of these once, in a numbered rule, so the R and Python
engines cannot read the same spec in two ways.

## Rows first, then columns

One specification produces one dataset. YAMAA first constructs the output
rows, then derives columns onto those rows. **Column derivation never changes
the row count.** See [Derivation](derive.md) for the full keys / rows /
columns walkthrough with diagrams.

A column may be derived at column level (one derivation for every row) or at
row level (in **every** row template, so each section can differ) -- never
both.

## A closed vocabulary that fails loudly

Derivations use only registered verbs -- `source`, `literal`, `mapping`,
`compute`, `case`, and about twenty more. There is no free-text derivation
column, no "you know what I mean." Every line is either valid or caught by
validation.

And every verb declares what happens when the data does not cooperate:

```yaml
derivation:
  mapping:
    source: DM.SEX
    dict: {M: M, F: F}
    missing: U      # "if not collected, set to U"
    unmapped: U     # "if not in codelist, set to U"
```

**Omit a handler and its condition is fatal.** Nothing quietly produces a `.`
and a NOTE in the log. "Not collected" and "collected but unrecognised" are
always two questions, and both must be answered in writing.

## What YAMAA deliberately does not have

Each of these is a recorded decision, not an omission:

| What you want to write | The answer |
|---|---|
| `ROUND(x, 1)` | Absent. Rounding is a reporting decision, not a derivation one |
| Arithmetic in a predicate, e.g. `when: "AVAL - BASE > 10"` | Invalid. Bind the value to a column first |
| A Boolean column | No Boolean type. A flag is `str` plus `allowed_values: [Y]` |
| `MAX(SUM(...))` | Reductions do not nest. Two levels means two specifications |
| A data-driven column count (SMQ01 ... SMQ0n) | The column list is fixed by the spec |
| Mixing R and Python functions in one project | `runtime.language` is project-wide |

For the full list, see the [Rules](../reference/rules.md). For where each of these lives in
a specification, see [Spec anatomy](spec-anatomy.md).
