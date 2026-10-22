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
specification with the same inputs always produces the same dataset, whether
the R package, the Python package or the `yamaa` command line runs it.

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
| **Structure** | the schema under `yaml/` -- what each file type may contain |
| **Algorithm** | your specification (`type: spec`) -- one specification produces one dataset |
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
| Schema | Declares what each file type may contain; anything it does not declare is rejected before execution. | [Schema introduction](schema-intro.md) |
| Rules | Fix the meaning of every written item, so no host can read the same spec in two ways. | [Rules](rules.md) |
| Engine | Runs specifications from Python, R and the command line; the same specification with the same inputs produces the same output dataset. | [R and Python API](api.md) |
| Benchmark | Runnable specifications with input data and byte-exact expected outputs. | [Benchmark](../benchmark/index.md) |

## What a specification looks like

One specification produces one dataset. yamaa first constructs the output
rows, then derives columns onto those rows. **Column derivation never changes
the row count.** See [Derivation](derive.md) for the full keys / rows /
columns walkthrough.

Every yamaa file starts with the same two fields: `yamaa`, the oldest
language version the file needs, always quoted, and `type`, the kind of file
it is. This specification builds SDTM DM from a raw table:

```yaml
yamaa: "1.0"
type: spec
model: SDTM
domain: DM
keys: [STUDYID, USUBJID]
input:
  RAW: raw/dm.csv

output:
  path: dm.csv
  columns: [STUDYID, DOMAIN, USUBJID, SEX]

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: RAW.STUDYID

  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: "'DM'"      # quoted: the text DM, not a column

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: RAW.USUBJID

  - name: SEX
    type: str
    label: Sex
    derivation:
      mapping:
        source: RAW.SEX
        dict: {M: M, F: F}
        missing: U      # "if not collected, set to U"
        unmapped: U     # "if not in codelist, set to U"
```

Derivations use only expressions and registered verbs -- no free-text
derivation column. In an expression, a bare name such as `RAW.STUDYID` reads
a column, and text is quoted, so `"'DM'"` is the text DM. A verb's value
fields, such as `dict` and its handlers, hold only constants, so `U` there is
the text U.

Every verb declares what happens when the data does not cooperate. **Omit a
handler and its condition stops the run.** "Not collected" and "collected but
unrecognised" are two conditions, answered by `missing` and `unmapped`. Write
`null` as the answer to leave the value missing instead.

## Four file types

Every file declares one of four types in its `type` line:

| Type | What one file holds |
|---|---|
| `spec` | One dataset: its inputs, keys, rows, columns, checks and output, as above. A specification may inherit from a `parent` layer, which is a `spec` file too. |
| `project` | A study: the specifications of each section (`sdtm`, `adam` or `send`), the codelists and function files they share, the lock of each host language, and the metadata define.xml needs. |
| `codelists` | The codelists of one terminology source, such as a CDISC Controlled Terminology release or the sponsor's own lists. |
| `function` | One project function: its parameters, return type and tests, and the callable that implements it in each host language, so R and Python share one contract and one set of tests. |

An engine reads a file that declares its own language version or an older 1.x
one, and refuses a newer file with one condition that names the version the
file needs.

## Working in plain text

Every input to a run is a file named in YAML, so the project directory is the
unit of review: people and agents edit its files, and one call runs them, such
as `yamaa run project.yaml`. Four calls help along the way, and none of them
reads study data:

| Step | Command line | R | Python |
|---|---|---|---|
| Lint, and fix the layout | `yamaa lint --fix PATH...` | `yamaa_lint(paths, fix = TRUE)` | `yamaa.lint(paths, fix=True)` |
| Get a file type's JSON Schema | `yamaa schema spec` | `yamaa_schema("spec")` | `yamaa.schema("spec")` |
| Find problems before any data | `yamaa check --format json PATH` | `yamaa_spec(path)$check()$issues` | `yamaa.spec(path).check().issues` |
| See the composed specification | `yamaa resolve PATH` | `yamaa_spec(path)$resolve()` | `yamaa.spec(path).resolve()` |

- **Lint** puts each file in one canonical layout and keeps its comments, so
  an edit by a person or an agent shows as the smallest diff.
- **Schema** gives the JSON Schema of the `spec`, `project`, `codelists` or
  `function` type; an editor that reads it validates and completes a file as
  it is typed.
- **Check** reports each problem it can find without data: its condition, the
  rule it breaks, its spec path and a message, and its file, line and column
  once the engine locates them. Problems that need the data, such as a column
  no input has, appear when the specification runs.
- **Resolve** shows the specification after its layers are composed, and
  where each field came from.

Sign-off happens on the YAML diff, with comments carrying the rationale next
to the fields they explain. The [R and Python API](api.md) describes the R
and Python calls and [the command line](api.md#command-line).

## Programs in a study submission

The sponsor chooses the program format and its location in the submission
package with its submission team. The program can be the YAML specification
with the driver that runs it, an R or Python script or a `yamaa` command, or
a readable R or Python implementation, including code produced with AI
assistance. The sponsor prepares and reviews any separate implementation as
part of its own development process.

Retain the exact inputs, specification and inherited files, project file,
codelist files and other referenced resources, function files with the
packages that implement them, the lock files the project names, and the
driver used for the build. Record the yamaa version and the host and
dependency versions. The sponsor determines which of these accompany the
submitted programs alongside the datasets, define.xml and reviewer's guide.

Verify a separate implementation independently against the yamaa results.
Match records by their declared keys and compare every output cell, including
missing values and types; state any numeric tolerance in the comparison
record. Retain the comparison report and resolve discrepancies before using
the implementation as the submitted program. Use a separately implemented
derivation for this verification: the R and Python packages and the command
line share one engine.

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
