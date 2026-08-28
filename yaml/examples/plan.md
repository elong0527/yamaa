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
| Companion columns negated to fake a descending sort | 2 specs | `order_by` has no direction |
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

### ~~S2. Numeric operands accept a variable or a literal~~ (superseded)

Superseded by `compute` and [R010](../rules/R010-scalar-computation.md), which
went further than this item proposed. Rather than widening each operator's
operand types, one expression takes a closed numeric formula, and `multiply`,
`add`, `subtract`, and `percent_change` were **deleted**.

The evidence this item cited is resolved. `sdtm-vs-unit-standardization` now
converts Fahrenheit with `(VSORRESN - 32) * 5 / 9`, and
`adam-adlb-closest-visit` reads the target day from the column that publishes
it, `ABS(ADY - AWTARGET)`.

Widening operands would have left one operator per column and one column per
step. The formula was the cheaper change: one registry entry, no new dependency
machinery, because R001 already extracted identifiers from SQL predicates.

### ~~S3. Ordering declares direction and null placement~~ (landed)

Landed as the `order_by_term` union in `schema_expression_core.yaml`, with the
ordering semantics stated once in [R007](../rules/R007-expression-registry.md)
and referenced by R003 for `multiple_matches`.

`NEGADY` and `NEGSEVN` are deleted, which were the suite's last two workaround
columns. Both golden outputs were checked unchanged: the ranking each fixture
depends on is reproduced exactly by the declared order terms.

R007 fixes `nulls` at `last` by default and, unlike PostgreSQL, does not flip
it under `desc`. That detail is the point of the item rather than a footnote:
engines disagree, and an implementation inheriting its engine's convention
would select a different record and still look conformant.

```yaml
order_term:
    - variable: {type: variable, required: true}
    - direction: {type: str, required: false, default: asc, values: [asc, desc]}
    - nulls: {type: str, required: false, default: last, values: [first, last]}
```

`row_number.order_by` and `multiple_matches.order_by` became
`list[order_by_term]`. A bare string keeps its meaning, so the other nineteen
`order_by` declarations in the suite are untouched.

Declaring `nulls` also removed the reason `adam-adae-worst-severity` had to be
built so that no partition holds two ineligible records. Its README said that
was "a property of the data, not a guarantee of the specification"; it is now a
guarantee.

### ~~S4. Ordered selection accepts a filter~~ (landed)

`filter` is now declared by `min`, `max`, and `multiple_matches`, and by
nothing else. R003 states that it is a predicate over right-side records only
and that a right side emptied by filtering is an ordinary absent match, not a
handled condition. R008 states that the filter runs before ordering and that
the handler count reports only records where more than one match survived it.

Three fixtures used it. `adam-adsl-disposition` and
`adam-adsl-treatment-selection` now share one filter between the aggregate and
the ordered selections, so the reductions agree on eligibility by construction;
neither value changed. `adam-adsl-crossover-periods` selects each period
directly and drops the guard.

One prediction was wrong. This item was expected to change no golden output,
and `adam-adsl-crossover-periods` changed: `EXFIRST` and `EXLAST` existed only
to expose the workaround and are gone with it. Its exposure input also gained a
second administration in each period, so the ordering inside a filtered right
side does real work rather than picking the only candidate. Every date and
treatment value is unchanged.

### ~~R1. Arithmetic propagates missing~~ (landed, via R010)

R010 states it for `compute`, which is now the only arithmetic expression:
`NULL` propagates through every operator and function, and division by zero
fails rather than returning missing, so a specification chooses missing
explicitly with `NULLIF`.

The predicted saving was real. The defensive `case` branches are gone from
`adam-adsl-treatment-selection`, `adam-adsl-crossover-periods`,
`adam-adae-worst-severity`, `adam-adsl-bmi-compute`, and both branches of
`sdtm-vs-unit-standardization`, and no golden output moved.

## Tier 2: new expressions with committed evidence

Each has at least one fixture that needs it. None can land until its failure
behavior is fixed by a negative fixture.

| Expression | Shape | Evidence | Decision it forces |
|---|---|---|---|
| ~~`divide`~~ | — | superseded: `/` in R010's grammar | zero denominator **fails**; `NULLIF` chooses missing |
| ~~`round`~~ | — | rejected: R010 has no rounding function | derivations must not round at all |
| ~~`abs`~~ | — | superseded: `ABS` in R010's table | none |
| ~~`greatest` / `least`~~ | — | superseded for numbers: `GREATEST` / `LEAST` in R010's table | all-missing returns missing |
| `rank` / `dense_rank` | same fields as `row_number` | `adam-adae-worst-severity` cannot flag a tied set | tie semantics |
| `lookup` | `dataset`, `on: list[{left, right}]`, `value` | `sdtm-suppmh-qualifiers`, `sdtm-vs-visit-study-day` | duplicate right side and no match |

Four of the six are superseded by one registry entry, which is the argument for
preferring a closed grammar over an expression per operator.

`round` was right to be singled out, and the answer went further than a mode
field. R and Python both round half to even by default while SAS rounds half
away from zero, so a `round` inheriting the host language disagrees across
runtimes on exactly the values a reviewer checks. R010 resolves this by
**omitting rounding entirely**: a derivation carries full precision and the
number of places shown is decided when the value is reported. There is no mode
to pin and no fixture that can round. `CEIL`, `FLOOR`, and `TRUNC` remain,
because an integral part is exact and has no mode.

What this does not settle is float-to-string conversion, which is R005's
question and now has three fixtures disagreeing with full-precision
arithmetic in their golden output.

`GREATEST` and `LEAST` close the row-wise maximum only for numbers.
`sdtm-dm-reference-dates` compares dates and still needs its null-guarded
branches.

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

1. ~~**R1, S2.**~~ Landed as `compute` and R010, which deleted the four
   arithmetic keywords rather than widening them. Seven fixtures were rewritten
   and every converted expression was checked bit-identical to the one it
   replaced, so no golden output moved.
2. ~~**S4.**~~ Landed. One golden output changed after all, in
   `adam-adsl-crossover-periods`; the other two fixtures kept every value. The only Tier 1 item left.
3. ~~**S3.**~~ Landed. `NEGADY` and `NEGSEVN` are gone, both fixtures keep
   their golden output, and both READMEs were rewritten. The
   `severity-order-completeness` verification went with `NEGSEVN`: it paired an
   output column with the internal one and had nothing left to assert.
4. ~~**S1.**~~ Done ahead of the other Tier 1 items. Eleven fixtures lost 28
   columns and eleven READMEs were revised. One change was not mechanical:
   `adam-adsl-dependency-order` marks `RANDFL` internal so that an output
   column depends on a non-output one, which is the case that detects an
   implementation building its dependency graph from output columns alone.
5. **Tier 2**, one expression at a time, each with its negative fixture. Only
   `rank`/`dense_rank` and `lookup` remain.
6. **Tier 3**, design documents before schema changes.

These steps also remove text from `README.md`: gaps 1, 2, 3, 10, and 16 are
retired by `compute`, gap 6 by S1, the right-side half of gap 13 by S4, and
gaps 4, 14, and 18 by S3.

## Negative fixtures this plan requires

The acceptance rule at the end of this file needs failure behavior fixed before
a feature is added. None of these exist yet, and they are the real gate on
Tier 2.

| Fixture | Provokes |
|---|---|
| non-output column named in `keys` | S1's only new error |
| unguarded division by zero, and the same expression guarded by `NULLIF` | R010's failure conditions |
| `SQRT` of a negative value, `LN` of zero, integer overflow | the rest of R010's failure conditions |
| an expression using `SUM`, `LAG`, a comparison, or a qualified identifier in the column phase | R010's closed grammar |
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
