---
title: Spec anatomy
---

# Spec anatomy

## A specification, annotated

Every `spec.yaml` follows the same shape:

```yaml
schema_version: "1.0"          # which schema version this spec targets
domain: ADSL                   # one dataset per spec
keys: [STUDYID, USUBJID]       # the grain: what identifies one row
parents: spec_study.yaml       # optional: inherit shared layers (Specification composition)

input:                         # every input is declared and named
  SOURCE: input/adsl.csv       # a CSV is typeless: fields default to str
  ADSL: {path: input/adsl.csv, types: {AVAL: float}}
base: SOURCE                   # the dataset that drives the row count

output:                        # the entry file must declare this completely
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGEGR1]   # what ships, in which order

intermediates:                 # optional: one named record several columns share
  - id: DEATHEV
    dataset: AE
    source: [STUDYID, USUBJID]
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"

columns:                       # dependency order; later columns may read earlier ones
  - name: AGEGR1
    type: str                  # str | int | float | date | datetime (closed set)
    label: Pooled Age Group 1
    derivation:                # how the value is produced (see below)
      cut:
        source: DM.AGE
        breaks: [18, 65]
        labels: ['<18', '18-64', '>=65']
        missing: UNKNOWN
    metadata:                  # for define.xml; never validated
      origin: Derived
    verifications:             # executed checks, not documentation
      - allowed_values: {values: ['<18', '18-64', '>=65', UNKNOWN]}

rows:                          # optional: only when one source record becomes several rows
  - id: alt
    filter: "LB.LBTESTCD = 'ALT'"
    derivations:
      PARAMCD: {literal: ALT}

verifications:                 # dataset-level assertions
  - unique: {columns: [STUDYID, USUBJID]}
```

Two splits to remember:

- **`columns` order vs `output.columns`.** `columns` is dependency order (BMI
  must be computed *after* HEIGHTCM exists); `output.columns` is delivery
  order. Excel has one `Variable Order` doing both jobs.
- **`metadata` vs `verifications`.** `metadata` is text for define.xml and
  nothing validates it; `verifications` actually runs and fails the run when
  violated. Excel's Length column often becomes both: `metadata.length` for
  humans and a `max_length` verification for the machine.

## Four words: class, type, expression, registry

You need these four to read the schema files; you never write them in a spec.

- **A class is a header definition.** It says which fields a mapping may
  contain and which are required -- exactly like a header row and the rows
  filled in under it. `column_class` declares `name` and `type` required and
  the rest optional; each entry under `columns:` is one row written under it.
  Writing a field the class does not declare fails validation instead of
  being silently ignored.
- **`type` lives in three namespaces**: the descriptor keyword inside
  `schema*.yaml`, the declared `column.type` in your spec (the closed five),
  and the runtime type a value carries while it is evaluated (never written).
- **A `derivation` is the column field that says how a value is produced. An
  `expression` is the one registered keyword that produces it.** A derivation
  is an expression plus what happens when it goes wrong (`missing`,
  `unmapped`, `conversion_failure`, ...). An expression is a mapping with
  exactly one entry: the key is the registered verb, the value is its
  parameters.
- **A registry is the list of permitted verbs.** Adding a verb costs one
  registry entry plus one rule -- deliberately, so the vocabulary cannot grow
  without end.

## The verb table

The closed derivation vocabulary, by family:

| Family | Verbs | What they do |
|---|---|---|
| Selection | `source`, `literal`, `first_available`, `greatest`, `least`, `case` | Read a variable, a fixed value, the first non-missing, or a conditional branch |
| Vocabulary | `mapping`, `lookup`, `cut` | Inline dictionary, a declared dataset, numeric banding |
| Strings | `str_extract`, `str_concat`, `str_template`, `str_upper`, `str_lower` | One regex group, concatenation, interpolation, case conversion |
| Arithmetic | `compute` | A closed grammar: `+ - * /`, parentheses, 13 functions (`ABS` `CEIL` `FLOOR` `TRUNC` `SQRT` `POWER` `EXP` `LN` `MOD` `GREATEST` `LEAST` `NULLIF` `COALESCE`). No `ROUND`, no `LOG` |
| Dates | `date_diff`, `study_day`, `date_impute`, `date_precision` | Whole-unit durations, CDISC study day, completing a truncated ISO date, reporting how much of it was collected |
| Windows | `row_number`, `rank`, `row_value`, `previous_non_missing`, `baseline_flag`, `baseline_value` | Numbering, ranking, reading neighbours, flagging and broadcasting a baseline -- all over constructed output rows |
| Reduction | `aggregate` | `SUM` `COUNT` `MIN` `MAX` `MEAN` `ONLY` -- reducing by the applicable keys unless `group_by` says otherwise |
| Extension | `function` | The single extension point: the spec names a logical contract, the project `environment.yaml` binds it to a versioned implementation |

Two composition rules shape how verbs combine: **operands are named variables,
not nested expressions** (bind a value to a column, then reference the name),
and nesting is permitted in exactly two places (`case` branches and
`str_concat.sources`). Dependencies stay visible without unfolding an
expression tree.

## Where everything lives

| If you already have | It maps to |
|---|---|
| One `.xlsx` with several sheets | One `spec.yaml` per `domain` -- each spec produces exactly one dataset |
| The Variable sheet's header row | `column_class` in `schema.yaml` |
| A row of the Variable sheet | One entry under `columns:` |
| Copying the company template and editing it | `parents:` layer inheritance (Specification composition) |
| The company macro library | `environment.yaml` (Project functions), validated separately from any spec |
| The worked examples in SDTMIG / ADaMIG | `benchmark/` -- the same illustrative role, except every benchmark runs |
| "We can't express that one -- let's discuss it" | A `benchmark/negative-*/` directory that pins the rejection |

The full schema file index is at [Schema reference](../reference/schema.md);
the execution semantics behind every verb are owned by the
[Rules](../reference/rules.md).
