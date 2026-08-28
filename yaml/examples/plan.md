# Plan for schema improvements driven by the example suite

## Purpose

The example suite is complete for Priority 1 and Priority 2 of the earlier
assessment plan: 32 fixtures, each covering one derivation boundary with
committed golden output. Their READMEs record 26 design gaps, grouped by root
cause in [`README.md`](README.md).

This file turns those findings into an ordered set of schema and rule changes.
Every item names the evidence that justifies it, the exact change, what it
breaks, and the negative fixture the acceptance rule requires before it can
land.

## What the fixtures cost today

Measured across the committed specifications rather than estimated:

| Workaround | Occurrences | Root cause |
|---|---|---|
| Columns emitted only to hold an intermediate value | 25 columns in 11 specs | no named intermediates |
| `case` branches that exist only to guard arithmetic against missing input | 8 purely defensive branches, more partially | arithmetic missing policy undefined |
| Companion columns multiplied by `-1` to fake a descending sort | 2 specs | `order_by` has no direction |
| A selection made correct by guarding it with an unrelated column | 3 specs state this explicitly | ordered selection cannot filter |

Most of the awkwardness in the suite comes from four constraints, and three of
them are restrictions on constructs that already exist rather than missing
features.

## Tier 1: relaxations of existing constructs

These change no derivation semantics that a golden output has already fixed.

### S1. A column may be excluded from the output (landed)

Implemented. Twenty-eight columns across eleven fixtures are now internal.

```yaml
column_class:
    - name: {type: str, required: true}
    - output: {type: bool, required: false, default: true}
```

A non-output column is derived, converted, and available to dependents exactly
as today; it is not written to the artifact. R005 needs two additional
sentences: `keys` must not name a non-output column, and column verifications
may still be declared on one.

This is the only Tier 1 item that introduces a new error, and its negative
fixture does not exist yet. An internal column named in `keys` must fail; that
behavior is currently asserted by R005 and untested.

### S2. Numeric operands accept a variable or a literal

The current asymmetry is sharper than the gap list records. `subtract` and
`percent_change` take variable and variable; `add` and `multiply` take variable
and literal. Neither form accepts a mix, which is why
`sdtm-vs-unit-standardization` converts Fahrenheit with `add` and an `addend` of
`-32`, and why `adam-adlb-closest-visit` writes target day `15` both as a column
and as a literal inside the same derivation.

```yaml
numeric_operand: {type: [float, variable]}

add:
    - source: {type: numeric_operand, required: true}
    - addend: {type: numeric_operand, required: true}
subtract:
    - minuend: {type: numeric_operand, required: true}
    - subtrahend: {type: numeric_operand, required: true}
multiply:
    - source: {type: numeric_operand, required: true}
    - factor: {type: numeric_operand, required: true}
```

Every existing specification stays valid, because a literal and a variable are
both still accepted where they were before.

### S3. Ordering declares direction and null placement

Deletes the negation columns in `adam-adlb-closest-visit` and
`adam-adae-worst-severity`, and closes the undefined-missing-order gap in the
same change.

```yaml
order_term:
    - variable: {type: variable, required: true}
    - direction: {type: str, required: false, default: asc, values: [asc, desc]}
    - nulls: {type: str, required: false, default: last, values: [first, last]}
```

`row_number.order_by` and `multiple_matches.order_by` become
`list[order_term]`. A bare string keeps its current meaning, so no existing
specification changes.

Declaring `nulls` also removes the reason `adam-adae-worst-severity` must be
built so that no partition holds two ineligible records.

### S4. Ordered selection accepts a filter

```yaml
multiple_matches_class:
    - order_by: {type: "list[order_term]", required: true}
    - keep: {type: str, required: true, values: [first, last]}
    - filter: {type: sql, required: false}
```

Three fixtures name this as their blocker. The semantics already exist: R003
defines right-side reduction by `filter` for `min` and `max`, and this applies
the same reduction before ordering. `adam-adsl-crossover-periods` would then
select a period directly instead of depending on an unrelated guard column, and
`adam-adsl-disposition` could restrict ordering to disposition events.

### R1. Arithmetic propagates missing

A rule change in R007 with no schema change. State that `add`, `subtract`,
`multiply`, `divide`, and `percent_change` return missing when any operand is
missing, and permit an optional local `missing` handler for the cases that want
a substitute.

Eight defensive `case` branches disappear and no golden output moves, because
the guarded expressions already produce missing today.

## Tier 2: new expressions with committed evidence

Each has at least one fixture that needs it. None can land until its failure
behavior is fixed by a negative fixture.

| Expression | Shape | Evidence | Decision it forces |
|---|---|---|---|
| `divide` | `numerator`, `denominator` | `sdtm-vs-unit-standardization` writes `5/9` as a decimal literal | zero denominator returns missing, following the `percent_change` precedent |
| `round` | `source`, `digits`, `mode` | `175 LB` standardizes to `79.37866475 kg` | rounding mode must be declared, not inherited |
| `abs` | `source` | `adam-adlb-closest-visit` spells absolute value as a `case` | none |
| `greatest` / `least` | `sources: list[variable]` | `sdtm-dm-reference-dates` writes a three-way maximum as null-guarded branches | whether all-missing returns missing or fails |
| `rank` / `dense_rank` | same fields as `row_number` | `adam-adae-worst-severity` cannot flag a tied set | tie semantics |
| `lookup` | `dataset`, `on: list[{left, right}]`, `value` | `sdtm-suppmh-qualifiers`, `sdtm-vs-visit-study-day` | duplicate right side and no match |

`round` deserves particular care. R and Python both round half to even by
default while SAS rounds half away from zero, so a `round` that inherits the
host language will disagree across runtimes on exactly the values a reviewer
checks. The mode must be an explicit field, and `half_up` is the expected
clinical default.

`lookup` replaces the two `mapping_from` calls in `sdtm-vs-visit-study-day` and
makes the SUPPQUAL parent join expressible. It returns one column per call; a
multi-column return conflicts with one expression producing one value and should
be considered separately.

## Tier 3: structural changes needing design first

These are design documents, not registry entries, and none should be attempted
as an incremental schema edit.

- **Date and datetime types with precision.** `adam-adae-partial-dates` rebuilds
  dates with regular expressions and string defaults, and
  `sdtm-ae-effective-transaction` carries a timestamp as `str`. Needs a type
  split, a precision concept, a declared imputation rule, and a statement about
  comparison under uncertainty.
- **Output row ordering.** `sdtm-suppmh-qualifiers` leaves rows in row-template
  order, which is not a submission order. Needs a root ordering declaration and
  a rule about its interaction with sequence assignment.
- **The multi-output pipeline.** `sdtm-suppmh-qualifiers` cannot assign and
  consume a parent sequence in one run, and `sdtm-dm-reference-dates` depends on
  an execution order it cannot state. Needs a manifest, cross-specification
  dependency inference, and cycle reporting.
- **Governed metadata.** `sdtm-dm-metadata-contract` declares origin, length,
  and codelist as free-form strings. Needs a vocabulary, a link between a
  declared codelist and its enforced `allowed_values`, and an expected metadata
  artifact.
- **Declarable study structure.** Group G in [`README.md`](README.md).
  Applicability, treatment period, relationship degree, and analysis window are
  protocol structure that the schema currently forces into filters and literals.
  This is the largest and least explored area.

## Sequencing

Two Tier 1 items are breaking to the fixtures, so the order matters.

1. **R1, S2, S4.** No golden output changes. Land first and confirm the suite
   still reproduces byte for byte.
2. **S3.** Two fixtures lose a negation column. Regenerate
   `adam-adlb-closest-visit` and `adam-adae-worst-severity`, and update both
   READMEs, which currently describe the workaround as necessary.
3. ~~**S1.**~~ Done ahead of the other Tier 1 items. Eleven fixtures lost 28
   columns and eleven READMEs were revised. One change was not mechanical:
   `adam-adsl-dependency-order` marks `RANDFL` internal so that an output
   column depends on a non-output one, which is the case that detects an
   implementation building its dependency graph from output columns alone.
4. **Tier 2**, one expression at a time, each with its negative fixture.
5. **Tier 3**, design documents before schema changes.

Steps 2 and 3 also remove text from `README.md`: gaps 4, 6, 14, and 18 are
retired by S1 and S3, and gap 16 by R1.

## Negative fixtures this plan requires

The acceptance rule at the end of this file needs failure behavior fixed before
a feature is added. None of these exist yet, and they are the real gate on
Tier 2.

| Fixture | Provokes |
|---|---|
| non-output column named in `keys` | S1's only new error |
| division by zero, with and without a handler | `divide` |
| a value exactly half way between two rounded results | `round` |
| `greatest` over columns that are all missing | `greatest` |
| `lookup` with a duplicate right-side key, and with no match | `lookup` |
| a specification declaring `direction: desc` on a column of mixed types | S3 |

They belong to the N-series of the earlier assessment plan, which remains
entirely unimplemented. Until they exist, every fail-closed claim in the fixture
READMEs is an assertion rather than a tested behavior.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
fixture needs it, a negative or edge fixture fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
fixtures should become closed, documented expressions instead.
