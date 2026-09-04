---
title: Why YAMAA
---

# Why YAMAA looks the way it does

> **YAMAA docs:** [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md)

> **Read this if** you want to know what an Excel specification cannot say, why
> that matters, and what YAMAA refuses to do about it.

---

## 1. The idea in one table

An Excel specification has three layers, but only two of them are written down:

| Layer | What it answers | Where an Excel spec keeps it | Who can read it |
|---|---|---|---|
| **Structure** | What exists -- which datasets, which variables, which types | Dataset sheet, Variable sheet | People, and machines with effort |
| **Algorithm** | What to compute | The free-text derivation column | People only |
| **Semantics** | What that computation means **when the data does not cooperate** | **Nowhere.** It lives in each programmer's experience | Nobody -- you cannot read what is in someone else's head |

YAMAA writes each layer into a different kind of file:

| Layer of YAMAA | File | Excel counterpart |
|---|---|---|
| **Structure** | `yaml/schema*.yaml` | The **header row** of the Variable sheet |
| **Algorithm** | your `spec.yaml` | The **body** of the Variable sheet, derivation column included |
| **Semantics** | `yaml/rules/R0xx-*.md` | The conventions that were never written down |
| **Worked examples** | `yaml/examples/` | The worked examples in an implementation guide -- but every one runs |

In one sentence: **an Excel spec is written to be understood; a YAMAA spec is
written to be executed the same way twice.**

That difference is what forces the third row into the open. A closed vocabulary
of derivation verbs ([the verb table](schema-concepts.md)) is how the algorithm layer becomes executable.
`rules/` is how the semantics layer becomes readable at all.

### 1.1 What "semantics" means in practice

If the word is too abstract, use the plain version: **the semantics layer is
the unwritten rules.** Concretely, it is everything that produces this
situation --

> Two programmers read the same specification. Both are certain they
> understood it. Their programs disagree.

The disagreement is almost never a misreading. It is that the specification
never reached the point they disagreed on. The three examples in the table
above are chosen because they sit at three different levels.

**"What missing means" -- the value level.** A specification says
`AVAL = LBSTRESN`. In the source, that cell might be an empty string, `NA`,
`.`, or `NOT DONE`. Which of those is missing? And one level deeper: *the
variable does not exist* and *the variable exists but this row is empty* are
two different situations -- the first is usually a broken extract, the second is
ordinary clinical reality -- yet an Excel spec calls both of them "missing".
YAMAA separates them: R014 owns which stored fields are missing, and R008 makes
`missing` on a `source` binding mean "the variable or ODM item does not exist in
context", while `missing` on any other expression means "the input value is
missing".

**"What a duplicate means" -- the row level.** A specification says
`TRTSDT: Predecessor ADSL.TRTSDT, merge by USUBJID`. What if a subject has two
ADSL records? A SAS merge will silently keep the last one, or produce a
cartesian product, depending on whether the programmer wrote `if a and b` and
on whether the data is genuinely one-to-many. **Nothing raises an error and
nothing leaves a mark**; the row count can change without anyone noticing. One
programmer's habit is to run `proc sort nodupkey` first; another's is to scan
the log for a NOTE. Neither habit is in the specification. R003 answers it
directly: multiple matches **fail by default**, and permitting them requires an
explicit `multiple_matches` that declares the ordering and whether to keep the
first or the last.

**"How dates compare" -- the type level.** `ASTDT >= TRTSDT` happens to work as
a string comparison while both sides look like `"2024-01-10"` -- until one is
`"2024-1-10"`, or a partial date `"2024-01"`. And a date is a number in SAS, a
`Date` in R, and a `datetime64` in Python, each with its own behavior when a
missing value enters a comparison. R016 therefore owns both temporal types
completely: which text becomes a date, how two of them order, what canonical
text they are written back as, and which operations may read them.

#### "Nobody can read it" is precise

It does not mean nobody knows. The opposite: **everybody has a copy of this
layer in their head.** The problem is that it cannot be cited, reviewed,
diffed, or inherited. There is no authoritative version to point at, no way to
see what changed between last year's convention and this year's, and no way to
keep it when the person who held it leaves.

There is a useful corollary. **Double programming is expensive precisely
because it is a way of reconstructing this layer.** What two independent
implementations expose is rarely a typo; it is two different assumptions about
what one sentence meant. Once the semantics layer is written down, double
programming has nothing left to compare but typos.

#### The questions an Excel spec never reaches

Every row below reads unambiguously in a specification and still produces
different programs:

| The question a spec does not reach | Why implementations diverge | Who answers it in YAMAA |
|---|---|---|
| Where do missing values sort? | Missing is smallest in SAS, so it sorts first; SQL engines disagree with each other; R defaults to `na.last = TRUE` | R007: `nulls` defaults to `last` and **does not flip with `desc`** |
| `100 * (AVAL - BASE) / BASE` when `BASE` is 0 | SAS yields missing plus one NOTE in the log; SQL raises; R yields `Inf` | R010 makes division by zero a failure; write `NULLIF(BASE, 0)` if missing is the intended answer |
| "Rounded to 1 decimal", at exactly `.5` | SAS `round()` is half-up; R `round()` defaults to banker's rounding | R010 provides no `ROUND` at all |
| A filter `"AESER = 'Y'"` when `AESER` is missing | SQL three-valued logic drops the row; SAS `if` has its own missing-value rules | R004: only `TRUE` retains a row |
| Sorting a character column with accents, mixed case, or non-Latin script | Collation follows the host locale, so the same code changes answer on a different machine | R007: code-point order, and host locale collation **must not** be substituted |
| `AGE` declared numeric, but the source holds `"045"` or `"4.5"` | Parsers differ in strictness; a failure may yield missing or may raise | R011 fixes the conversion table cell by cell |
| A value the codelist does not contain | Keep the original? Blank it? `UNKNOWN`? Raise? | R008: without an `unmapped` handler the condition is fatal |
| Two results on the same day, both eligible as baseline | Each programmer picks differently | `baseline_flag` raises rather than choosing |
| Which order do the rows leave in? | Whatever the last sort in the program happened to be, or the input order | R005: `output.order_by` declares it, and rows equal on every term keep construction order |

The pattern is the same in every row: the divergent behaviors are all
*defensible*, several are *silent*, and the specification is what should have
chosen between them.

This is what the eighteen files in `rules/` are, and it is why each of them
owns exactly one topic and cross-references the others without restating them.
A restatement would be a second place to keep correct, which is how an
unwritten convention starts drifting in the first place.

---

---

## 2. The folder against a workbook

The fastest way in is to map what you already have onto what the folder
holds:

| Excel workbook | YAMAA |
|---|---|
| One `.xlsx` with several sheets | One `spec.yaml` per `domain` -- each spec produces exactly **one** dataset |
| The Variable sheet's header row | `column_class` in `schema.yaml` |
| A row of the Variable sheet | One entry under `columns:` in `spec.yaml` |
| Copying the company template and editing it | `parents:` layer inheritance (R017) |
| `Working-Instruction-fill-in-spec.docx` | `rules/` -- normative text an implementation cites, not advice |
| The company macro library | `environment.yaml` (R018), validated separately from any spec |
| The worked examples in SDTMIG / ADaMIG | `examples/` -- the same illustrative role, except every example runs and its output is fixed byte for byte |
| "We can't express that one -- let's discuss it" | An `examples/negative-*/` directory that pins the rejection |

The `negative-` directories are worth a slide of their own. They are not bad
examples; they **declare where the design refuses you**, and
`expected/error.yaml` fixes exactly which error is raised. An Excel spec has no
equivalent -- a spec that cannot be implemented is discovered by whoever tries.

---

---

## 3. What an Excel spec has that YAMAA deliberately does not

This section draws the most questions, so prepare for it. None of these is an
omission; each is a decision recorded in a rule.

| What you want to write | The answer | Rule |
|---|---|---|
| `ROUND(x, 1)` | Absent. Rounding is a reporting decision; use `FLOOR`/`CEIL`/`TRUNC` to choose an integer explicitly | R010 |
| `LOG(x)` | Absent; the base differs between dialects. Write `LN(x)` | R010 |
| Arithmetic in a predicate, e.g. `when: "AVAL - BASE > 10"` | Invalid. A predicate operand is a name or a literal; bind the value to a column first | R004 |
| `!=` | Invalid; write `<>` | R004 |
| A function call, `CASE`, or a subquery in a predicate | None of them are in the grammar | R004 |
| Length treated as a type | Length is a `max_length` verification | R009 / R011 |
| A Boolean column | No Boolean column type. A flag is `str` plus `allowed_values: [Y]` | R011 |
| `25.5` becoming `int` 26 | Fails. A non-integral value is neither truncated nor rounded | R011 |
| A result of `Inf` or `NaN` | Normalized to **missing**, immediately, at every boundary | R011 |
| A same-named source variable picked up automatically | Forbidden. Every column is derived explicitly; write `literal: null` for a deliberate blank | R002 / R005 |
| A nested expression in an operand field | Nesting exists in three places only (`case.then`, `str_concat.sources`, `override.value`); otherwise name an intermediate column | R007 |
| A computed argument in `function.args` | Invalid. Arguments are variable names or closed literal leaves | R018 |
| `MAX(SUM(...))` | Reductions do not nest. Two levels means two specifications with a stored intermediate | R013 |
| A data-driven column count (SMQ01 ... SMQ0n) | The column list is fixed by the spec. A dictionary that outgrows it is a spec change, not a data-dependent artifact schema | R005 |
| Repeating a row *n* times from data (e.g. expanding EXDOSFRQ into daily records) | Row construction never invents rows. Supply a planning relation with one driver record per required row; expansion happens upstream | R001 |
| Mixing R and Python functions in one project | `runtime.language` is project-wide | R018 |
| Ordering that follows the host locale | Forbidden; `str` orders by code point | R004 / R007 |

One more thing worth saying about the rules themselves: **all eighteen indexed
rules are normative -- none is draft.** The repository's admission policy is
that a design proposal may live in an issue or a branch, but it becomes a rule
only once its schema shape can be validated, its behavior is closed enough for
independent R and Python implementations, and examples exercise both success
and failure. **An implementation must not substitute an open proposal for the
indexed text.**

---
