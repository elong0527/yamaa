# Card text beyond D71

Part of the [1.0 interface RFC](../interface-1.0.md). The owner approved the
RFC's proposed card text as D71 (the [rules page](rules.md)). Some cards that
1.0 changes have no approved wording there, and `rules/README.md` makes a
change to a card's title or statement the maintainer's decision. This page
drafts that wording for the cards that question 1 of the
[Phase 1 and 2 plan](https://github.com/elong0527/yamaa/issues/3030#issuecomment-6275644998)
lists, for the owner's approval before P1-3 merges (#3162). Nothing here
changes a card: each Phase 1 pull request writes its cards, and their detail
and evidence get ordinary review.

Every proposed title is one sentence without a final period, and every
statement has at most 90 words, counted as `tools/rules/model.py` counts them;
each card gives its counts. Today's text is quoted from `rules/` on main, with
links shown as their text and rule IDs written without links. Today's wording
stays wherever no decision requires a change. A new card is named by its file,
topic and the next free number of its series, because a rule ID cannot be
cited before its card exists.

The cards describe the 1.0 surface. Under D75, from P1-3 until P2-14
`rules/README.md` carries one transitional sentence: a file that declares
`schema_version` is read under the rules at the `yamaa-v0.5.0` tag, and until
P2-13 a card keeps on its Fails line the conditions the 0.5 path still raises.
Each card below therefore gives its 1.0 Fails line, then the conditions it
keeps until P2-13.

The plan's list is not complete. The
[last section](#cards-the-plan-did-not-list) lists the other cards whose title
or statement names a 0.5 spelling or term, found by checking every card
against the approved rename table and decisions.

## Summary

| Card | Changes | What changes | Pull request |
| --- | --- | --- | --- |
| SCHEMA-2 | title, statement | `yamaa` and `type` replace `schema_version`; the minimum-version rule replaces the exact match (D4, P-VER-1) | P1-3 |
| LAYER-4 | title, statement | each layer admitted at its own version, declaring `type: spec` (D4, P-VER-1) | P1-3 |
| SPEC-1 | statement | the header and `model` replace `schema_version` (D4, D25) | P1-3 |
| COLUMN-10 | statement | a handler holds a value-field constant, and YAML null answers missing (D18, D31) | P1-4 |
| SCHEMA-15 | title, statement | one spelling is scoped to shorthand unions; bare keywords join (D18, P-HND-4) | P1-5 |
| TIME-10 | title, statement | `minimum_source_precision` goes; a `month` needs a `day` (D61, P-IMP-1) | P1-5 |
| LOOKUP-22 | title, statement | `derive` becomes `derivations` (P-BIND-1) | P1-5 |
| LOOKUP-24 | statement | the intermediate's `no_match` becomes `absent` (P-HND-1) | P1-5 |
| WINDOW-5 | title, statement | `baseline_flag` takes `window.order_by`, defaulting to its date (P-ORD-1) | P1-5 |
| PROJECT-1 | title, statement | functions and host locks replace environments; the `yamaa` header (D3, D4, D8) | P1-8 |
| PROJECT-2 | title, statement | `environments` holds each host's lock instead of an environment file (D8) | P1-8 |
| FUNC-1 | title, statement | function files and the running host's implementation replace environment files (D8, P-FN-2) | P1-8 |
| FUNC-2 | title, statement | the function file replaces the environment file (D4, D8, P-FN-2) | P1-8 |
| FUNC-3 | statement | the language is the running host's and the lock is in the project; title kept (D8) | P1-8 |
| FUNC-4 | statement | a listed function file per definition, with `name` and `implementations`; title kept (D8) | P1-8 |
| JSON-1 | title, statement | `datasets: json` becomes `transport: dataset_json` (P-PRJ-1) | P1-8 |
| XPT-1 | title, statement | `datasets: xpt` becomes `transport: xpt` (P-PRJ-1) | P1-8 |
| SUB-3 | title, statement | the specification declares `model`; the bound guide must belong to it (D25, P-MODEL-1) | P1-10 |
| SUB-23 | title, statement | every rule needing no project file runs on the specification alone (D25, P-MODEL-1) | P1-10 |
| New SPEC card (`copy`) | new card | the copy entry (D19, D70, P-COPY-1) | P1-12 |
| SUB-20 | title, statement | a bare reference or a copy entry makes the Predecessor copy (D18, D19) | P1-12 |
| SUB-14 | statement | `origins`, one or more, with `conflicting_origins` (D16, D70, P-ORIGIN-1) | P1-13 |
| DEFINE-8 | statement | `submission.standard` selects a section standard; the model check moves to SUB-3 (D16, D25) | P1-13 |
| DEFINE-19 | title, statement | one `def:Origin` per declared origin (D16, P-ORIGIN-1) | P1-13 |
| INPUT-1 | statement | `.xml` selects the ODM profile (D26, P-ODMIN-1) | P1-15 |

FUNC-3 and FUNC-4 are included because D8 changes their statements; their
titles stay. INPUT-1 is written by P1-15, outside the P1-3 to P1-13 range that
#3162 names.

## Questions for the owner

Each question names its recommendation, and the proposed text follows it.

1. **PROJECT-1: an unreadable or invalid project file (Q1).** Today they fail
   as `project_environment_missing` and `project_environment_invalid`. The
   rename table renames those to `function_implementation_missing` and
   `function_file_invalid`, which mean function-file faults, and says that an
   invalid project file "keeps PROJECT-1's conditions", which no longer exist
   under their names. **Recommendation:** no new condition: an unreadable
   project file fails with FILE-11's conditions, like any other file a run
   reads, and invalid content with the schema conditions of SCHEMA-12 and
   SCHEMA-13 at its project path. The alternative is a pair of project-file
   conditions, which D69 did not approve.
2. **TIME-10: a `month` without `day` (Q2).** Dropping
   `minimum_source_precision` (D61, P-IMP-1) retires the four conditions that
   checked the rules against it and leaves three combinations: both rules,
   `day` alone, or neither. A `month` without `day` can still never complete a
   date, since a source missing its month also misses its day.
   **Recommendation:** it fails validation as `missing_required_field` at the
   `impute`, as TIME-1 refuses a field its declared type requires. The
   alternative keeps `day_required` for this one case.
3. **WINDOW-5 and WINDOW-9: a declared `baseline_flag` order (Q3).** P-ORD-1
   admits `window.order_by` on `baseline_flag`, defaulting to the `date`, and
   folds `tie_break: [ATM, LBSEQ]` into `window.order_by: [ADT, ATM, LBSEQ]`;
   it does not say whether a declared order must start with the date.
   WINDOW-9's approved title (D71) says the baseline is the last eligible row
   "in date order". **Recommendation:** a declared order starts with the
   `date` operand ascending, and any other first term fails as
   `value_not_permitted`, which keeps that title true and the baseline the
   latest eligible date. The alternative admits any order and changes
   WINDOW-9's title.
4. **SUB-3: a specification under another model's section (Q4).** P1-10
   checks that a section lists only specifications of its model, but the
   rename table names `standard_model_mismatch` only for a bound guide outside
   the model. **Recommendation:** the same condition, with context naming the
   section. The alternative is `value_not_permitted` at the section's `specs`
   entry.
5. **The copy card: its file and conditions (Q5).** The card goes in
   `rules/specification.md`, which owns declaring columns (SPEC-3, SPEC-4,
   SPEC-7), rather than the submission profile, since a copy declares columns
   with or without submission metadata. P-COPY-1 names no condition, and D69
   approved none for `copy`. **Recommendation:** reuse `unknown_field` for a
   name that is not one of the producer's output columns, or a `submission`
   field other than `core`, `mandatory` and `role`; `duplicate_identifier`
   for a copied name another column declares (SPEC-2); and
   `prohibited_construct` for a `dataset` that names an input without
   `schema`.
6. **COLUMN-10's pull request (Q6).** The plan writes COLUMN-10's paragraph in
   P1-4 (D71) and its statement in P1-5. **Recommendation:** write both in
   P1-4, so the statement and its detail change together.

## P1-3: header, version rule and file types

### SCHEMA-2

`rules/reference/schema-language.md`; changes the title and statement.

Today (title 17 words, statement 48 words):

> A specification, project file or environment file declares in
> `schema_version` the bundle version it is written against
>
> A specification, project file or environment file declares its entry-point
> bundle version in `schema_version`, and the declared version must equal the
> bundle version. Any other version is an error, and the document is not
> validated against the bundle. An invalid include or an inconsistent bundle
> version also fails.

Proposed (title 19 words, statement 89 words):

> A yamaa file declares its type and the oldest language version it needs, and
> an older engine refuses it
>
> A specification, project file, codelist file or function file declares
> `yamaa`, the oldest language version it needs, quoted as `"1.N"`, and
> `type`: `spec`, `project`, `codelists` or `function`. An engine of language
> version 1.M reads a file declaring 1.M or an older 1.x version, and admits a
> feature only in a file declaring the version that introduced it or a later
> one. A file declaring a version the engine does not read fails as
> `unsupported_language_version`, and one whose `type` its call or referencing
> field does not expect fails as `unexpected_file_type`.

- **Follows:** D4, D10; P-VER-1, P-TYPE-1 and P-DICT-1 (D58); the conditions
  `unsupported_language_version` and `unexpected_file_type` (D69).
- **Fails:** `unsupported_language_version`, `unexpected_file_type`, and
  `unknown_field` or `value_not_permitted` for a field or value newer than the
  declared version (P-VER-1). Kept until P2-13: `schema_version_mismatch`.
- **Detail:** the closing paragraph gives way to the version rule's cases. An
  unquoted `1.0` is a YAML float (SCHEMA-4) and fails as `invalid_field_type`;
  a field or value newer than the version a file declares fails in that file
  as `unknown_field` or `value_not_permitted`, with context naming the version
  that introduced it (P-VER-1); `unexpected_file_type` is checked at each
  referencing field: a project's `functions`, `codelists` and section `specs`,
  a specification's `parent` and an input's `schema`. A dictionary file that
  `mapping.dict` names carries no header, like a data file (P-DICT-1). Today's
  last sentence, on includes and bundle versions, is about the shipped schema
  files and moves to SCHEMA-1's detail.
- **Evidence:** `negative-version-mismatch`, re-authored for its 1.0 fault
  (D60), and new negatives for a newer version and an unexpected `type`. The
  two Rust tests stay for the 0.5 path until P2-13.
- **Written by:** P1-3.

### LAYER-4

`rules/inheritance.md`; changes the title and statement.

Today (title 11 words, statement 56 words):

> Every layer is a valid fragment of the active schema version
>
> Each layer must be a non-empty mapping that declares `schema_version` equal
> to the active bundle version. A layer need not be complete: requiredness is
> checked on the resolved specification, but unknown fields and invalid values
> fail in the layer that writes them. Two members of one keyed collection in
> one layer must not share an identifier.

Proposed (title 12 words, statement 73 words):

> Every layer is a valid fragment of the language version it declares
>
> Each layer must be a non-empty mapping that declares `yamaa` and
> `type: spec` (SCHEMA-2), and is admitted against the language version it
> declares; the resolved specification records the newest version any layer
> declares. A layer need not be complete: requiredness is checked on the
> resolved specification, but unknown fields and invalid values fail in the
> layer that writes them. Two members of one keyed collection in one layer
> must not share an identifier.

- **Follows:** D4; P-VER-1 (D58).
- **Fails:** `unsupported_language_version`, `unexpected_file_type`,
  `duplicate_identifier`. Kept until P2-13: `schema_version_mismatch`.
- **Detail:** "A version mismatch fails before composition" becomes: a layer
  whose version the engine does not read fails before composition, reporting
  the file and its version. Layers may declare different versions, each
  admitted at its own, and a `clear:` path is checked against the version the
  resolved specification records (P-VER-1). Inheritance still never migrates
  versions.
- **Evidence:** `negative-version-mismatch` (D60). A chain whose layers
  declare different versions needs a feature newer than 1.0, so until a 1.1
  exists a unit test covers the comparison.
- **Written by:** P1-3.

### SPEC-1

`rules/specification.md`; changes the statement; the title stays.

Today (title 12 words, statement 69 words):

> A specification derives one output dataset from the root fields it declares
>
> A specification names the schema bundle it is written against in
> `schema_version` and derives the one output dataset `domain` names. Its
> other root fields declare the row identity (`keys`), the input datasets it
> reads (`input`), how output rows are built (`base`, `filter`, `rows`), the
> values derived for them (`windows`, `intermediates`, `columns`), the
> artifact (`output`), the assertions over the completed dataset
> (`verifications`) and the metadata carried with it (`submission`,
> `metadata`).

Proposed (title unchanged, statement 73 words):

> A specification derives one output dataset from the root fields it declares
>
> A specification declares `yamaa` and `type: spec` (SCHEMA-2), may name its
> CDISC model in `model` (SUB-3), and derives the one output dataset `domain`
> names. Its other root fields declare the row identity (`keys`), the input
> datasets it reads (`input`), how output rows are built (`base`, `filter`,
> `rows`), the values derived for them (`windows`, `intermediates`,
> `columns`), the artifact (`output`), the assertions over the completed
> dataset (`verifications`) and the metadata carried with it (`submission`,
> `metadata`).

- **Follows:** D4, D10, D25; P-MODEL-1 (D58).
- **Fails:** none, as today.
- **Detail:** "Schema language requires `schema_version` to match the bundle
  exactly" becomes: SCHEMA-2 governs the header, Inheritance resolves `parent`
  and applies `clear` (LAYER-9), and a `columns` entry may be a `copy` entry
  (the new SPEC card below). `model` is in the 1.0 schema from P1-1 and
  checked from P1-10.
- **Evidence:** none, as today.
- **Written by:** P1-3.

## P1-4 and P1-5: specifications

### COLUMN-10

`rules/columns.md`; changes the statement; the title stays.

Today (title 29 words, statement 78 words):

> A local handler answers its condition with a literal, and an omitted handler
> gives missing for a missing single input or absent record and leaves every
> other condition fatal
>
> Each local handler applies at the site the table below lists, only when its
> condition occurs, and substitutes a scalar literal or `{literal: null}`,
> never an expression. Omitted, or written as YAML null, a handler for the
> missing input of a single-input verb, a record a lookup does not find, an
> `UNKNOWN` flag condition or `first_available` sources all missing gives
> missing, counted as that handler. Every other condition fails without its
> handler, a missing `str_template` placeholder among them.

Proposed (title unchanged, statement 87 words):

> A local handler answers its condition with a literal, and an omitted handler
> gives missing for a missing single input or absent record and leaves every
> other condition fatal
>
> Each local handler applies at the site the table below lists, only when its
> condition occurs, and substitutes the constant it declares, written as a
> value field: text, a number or a Boolean, or YAML null for missing, never an
> expression. An omitted handler for the missing input of a single-input verb,
> a record a lookup does not find, an `UNKNOWN` flag condition or
> `first_available` sources all missing gives missing, counted as that
> handler. Every other condition fails without its handler, a missing
> `str_template` placeholder among them.

- **Follows:** D18, D31; D67 and P-HND-1 for the table.
- **Fails:** `missing_input`, `unmapped_value`, `unmatched_pattern`, as today.
- **Title:** it stays, since "a literal" still names the constant a handler
  holds.
- In the statement, YAML null now declares a handler that answers missing
  instead of leaving it absent, so `unmapped: null` answers missing where
  today it fails.
- **Detail:** the paragraph on `{literal: null}` is the one D71 approved:
  handler values are value fields, YAML null is the missing value and never
  removes a handler, and a child layer clears an inherited handler with
  `clear:` (LAYER-9). The table's "Stage" column becomes "Phase" (P-PHASE-1,
  D71), its bind row goes (D67), and `intermediates[].no_match` becomes
  `intermediates[].absent` (P-HND-1). A handler that restates its default is
  the lint finding `restated_default`, never a refusal (P-HND-3).
- **Evidence:** unchanged, converted in Phase 2.
- **Written by:** P1-4, with the paragraph D71 approved; the plan lists the
  statement under P1-5 (Q6).

### SCHEMA-15

`rules/reference/schema-language.md`; changes the title and statement.

Today (title 14 words, statement 79 words):

> A written long form that holds only the field its shorthand fills is refused
>
> A value has one spelling. A written value that matches the class of a
> shorthand union and holds nothing but the field a bare value of that union
> fills restates the shorthand and fails validation, reporting its path, that
> field and the shorthand to write instead. A long form that writes any other
> field, even a default or a null, keeps its meaning. The refusal applies to
> what an author wrote, never to the canonical form a validation produces.

Proposed (title 13 words, statement 87 words):

> A written long form that holds only what its shorthand fills is refused
>
> A value written through a shorthand union has one spelling. A written value
> that matches the class of a shorthand union and holds nothing but what a
> bare value of that union fills restates the shorthand and fails validation,
> reporting its path, the restated field if there is one, and the shorthand to
> write instead. A long form that writes any other field, even one restating a
> default, keeps its meaning. The refusal applies to what an author wrote,
> never to the canonical form a validation produces.

- **Follows:** D18; P-HND-4, P-LONG-1 and P-BIND-2 (D58).
- **Fails:** `redundant_long_form`, as today.
- **Title:** "the field" becomes "what": P-HND-4's bare keyword
  `- not_missing` fills no field, and its refused long form `not_missing: {}`
  holds nothing. The first sentence narrows to shorthand unions because an
  expression field admits a number, Boolean or missing written either as a
  YAML scalar or as grammar text; validation admits both, and lint's
  `scalar_as_text` rewrites the text form (P-KIND-1). "Even a default or a
  null" becomes "even one restating a default": YAML null no longer clears
  (LAYER-9).
- **Detail:** the table loses the `{source: X}` row (D18) and gains
  `- not_missing` against `not_missing: {}` (P-HND-4), and with P1-14 a
  value-level comparator class that restates its list (P-VLM-1). The paragraph
  that reads a bare string derivation as EXPR-5's source shorthand goes with
  EXPR-5's D71 text, and so does the sentence on a layer's clear marker.
- **Evidence:** unchanged; `negative-redundant-long-form` writes
  `aggregate: {expr: E}`, which stays refused.
- **Written by:** P1-5.

### TIME-10

`rules/temporal.md`; changes the title and statement.

Today (title 13 words, statement 83 words):

> impute's month and day rules must fit its precision policy and the calendar
>
> `month` is declared exactly when `minimum_source_precision` is `year`, and
> `day` exactly when it is `year` or `month`. A declared `month` must lie from
> 1 to 12 and a numeric `day` from 1 to 31, and the completed value must be a
> real calendar date. A `day` that names a position in its month is not a
> literal to range-check. Any other `day` token is neither a number nor a
> position and is rejected where the specification is read, before any data is
> seen.

Proposed (title 12 words, statement 73 words):

> impute's month and day rules must fit each other and the calendar
>
> A declared `month` requires a declared `day`. A declared `month` must lie
> from 1 to 12 and a numeric `day` from 1 to 31, and the completed value must
> be a real calendar date. A `day` that names a position in its month is not a
> literal to range-check. Any other `day` token is neither a number nor a
> position and is rejected where the specification is read, before any data is
> seen.

- **Follows:** D61; P-IMP-1 (D58).
- **Fails:** `missing_required_field` (Q2), `month_out_of_range`,
  `value_not_permitted`, `invalid_calendar_date`. Kept until P2-13:
  `month_required`, `month_not_permitted`, `day_required`,
  `day_not_permitted`.
- **Detail:** the paragraph on the four retired conditions becomes one
  sentence: a `month` without `day` fails as `missing_required_field` at the
  `impute` (Q2). Which sources `impute` completes follows from the rules
  supplied, as TIME-8 states: with `month` and `day` a year alone, with `day`
  alone a year and month, with neither only the time of a datetime. TIME-8's
  statement drops its `minimum_source_precision` sentence too; it is in the
  last section, since the plan did not list it.
- **Evidence:** `negative-impute-missing-month` and
  `negative-impute-unreachable-month` retire in P2-4 (D61) and remain 0.5-path
  evidence until then; a new negative for a `month` without `day`, authored
  independently.
- **Written by:** P1-5.

### LOOKUP-22

`rules/lookup.md`; changes the title and statement.

Today (title 11 words, statement 77 words):

> A qualified aggregate's derive bindings compute typed per-record values
> before reduction
>
> A qualified aggregate may declare `derive` to bind per-record intermediate
> variables before reduction. Each binding declares a `name`, a `type` and a
> `derivation`; bindings evaluate once per record of the aggregate's relation,
> in declaration order, and each derivation reads the record's fields and the
> values bound by earlier bindings. The reducer expression names each bound
> variable by its unqualified `name`. A binding cannot use a window operation,
> and `derive` is not available on unqualified or grouped-input aggregates.

Proposed (title 10 words, statement 79 words):

> A qualified aggregate's derivations compute typed per-record values before
> reduction
>
> A qualified aggregate may declare `derivations` to bind per-record
> intermediate variables before reduction. Each entry, keyed by the variable's
> name, declares its `type` and `derivation`; entries evaluate once per record
> of the aggregate's relation, in declaration order, and each derivation reads
> the record's fields and the values bound by earlier entries. The reducer
> expression names each bound variable by its unqualified name. An entry
> cannot use a window operation, and `derivations` is not available on
> unqualified or grouped-input aggregates.

- **Follows:** P-BIND-1 (D58).
- **Fails:** `invalid_derive_step`, `unknown_derive_variable`, as today.
- **Detail:** "derive binding" becomes "aggregate derivation", the path
  `aggregate.derive[N]` becomes `aggregate.derivations.<name>` (the rename
  table's `paths`), the map composes by key under LAYER-6, and "an `aggregate`
  or `compute` expression" becomes "an `aggregate` expression or a
  derivation's text" (D18).
- **Evidence:** unchanged, converted in Phase 2.
- **Written by:** P1-5.

### LOOKUP-24

`rules/lookup.md`; changes the statement; the title stays.

Today (title 14 words, statement 84 words):

> A lookup with no candidate answers absence, missing by default, unless
> require_match fails it
>
> A lookup with no candidate for a row, or with a missing match or range
> value, answers absence: an implicit join gives its `source` binding's
> `absent` value and a named intermediate its `no_match` value, either missing
> when undeclared and counted under that handler unless a predicate made the
> read. `require_match: true` on the binding or intermediate, or on an input
> for joins that declare no `absent`, instead fails the read as
> `unmatched_key`; one lookup cannot declare both. An aggregate's empty match
> yields missing (FORMULA-24).

Proposed (title unchanged, statement 85 words):

> A lookup with no candidate answers absence, missing by default, unless
> require_match fails it
>
> A lookup with no candidate for a row, or with a missing match or range
> value, answers absence: an implicit join gives its `source` binding's
> `absent` value and a named intermediate its own `absent` value, either
> missing when undeclared and counted under that handler unless a predicate
> made the read. `require_match: true` on the binding or intermediate, or on
> an input for joins that declare no `absent`, instead fails the read as
> `unmatched_key`; one lookup cannot declare both. An aggregate's empty match
> yields missing (FORMULA-24).

- **Follows:** P-HND-1 (D58); D67 for the detail.
- **Fails:** `unmatched_key`, `conflicting_absence_policy`,
  `prohibited_construct`, as today.
- **Detail:** `no_match` becomes `absent` throughout; `absent: null` answers
  missing like an omitted `absent`, and in a child layer it overrides an
  inherited answer, while `clear:` restores the default (LAYER-9). The
  sentence that `absent` keeps answering a variable or ODM item that does not
  exist goes (D67).
- **Evidence:** unchanged names; negative paths `intermediates[N].no_match`
  become `intermediates.<id>.absent` (the rename table's `paths`).
- **Written by:** P1-5.

### WINDOW-5

`rules/windows.md`; changes the title and statement.

Today (title 9 words, statement 58 words):

> Positional window operations require window.order_by, and baseline_flag
> forbids it
>
> `row_number`, `rank`, `row_value` and `locf` require `window.order_by`:
> without a declared order the window has no positions to number or to move
> along, and omitting it is a validation error. `baseline_flag` orders its
> eligible rows by `date` and then by its own `tie_break` terms, and does not
> take `window.order_by`; declaring one is a validation error rather than
> silently ignored.

Proposed (title 12 words, statement 54 words):

> Positional window operations require window.order_by, and baseline_flag
> defaults it to its date
>
> `row_number`, `rank`, `row_value` and `locf` require `window.order_by`:
> without a declared order the window has no positions to number or to move
> along, and omitting it is a validation error. `baseline_flag` also takes
> `window.order_by`, which defaults to its `date` alone; a declared order
> starts with that `date` ascending, and its later terms break ties
> (WINDOW-9).

- **Follows:** P-ORD-1 (D58).
- **Fails:** `window_order_by_required`, `value_not_permitted` (Q3). Kept
  until P2-13: `window_order_by_forbidden`.
- The clause "a declared order starts with that `date` ascending" is Q3's
  recommended answer; without it the last sentence ends at "its `date` alone".
  WINDOW-9's statement changes with it (last section).
- **Evidence:** the Rust test whose name says the order stays forbidden is
  rewritten; `schema-baseline-tie-break`, converted, shows `window.order_by`
  on `baseline_flag`.
- **Written by:** P1-5.

## P1-8: project and function files

### PROJECT-1

`rules/profiles/project.md`; changes the title and statement.

Today (title 15 words, statement 69 words):

> A project file is passed explicitly and names one study's environments,
> terminology and submission sections
>
> A project file names one study's implementation environments, terminology
> and submission sections. The caller passes it explicitly, and it is admitted
> against `schema_project.yaml`, whose `schema_version` selects the schema
> bundle. No project file is discovered from a specification, a directory or a
> host default, and a specification cannot select, replace or override its
> project. A run without a project file has no implementation environment, no
> terminology and no dataset inventory.

Proposed (title 17 words, statement 76 words):

> A project file is passed explicitly and names one study's functions, host
> locks, terminology and submission sections
>
> A project file names one study's function files and host locks, its
> terminology and its submission sections. The caller passes it explicitly; it
> declares `yamaa` and `type: project` (SCHEMA-2) and is admitted against the
> project schema. No project file is discovered from a specification, a
> directory or a host default, and a specification cannot select, replace or
> override its project. A run without a project file has no project functions,
> no terminology and no dataset inventory.

- **Follows:** D3, D4, D8; P-FN-2 and P-TYPE-1 (D58).
- **Fails:** as Q1 decides; recommended: none of its own, an unreadable
  project file failing under FILE-11 and invalid content under SCHEMA-12 and
  SCHEMA-13. Kept until P2-13: `project_environment_missing`,
  `project_environment_invalid`.
- **Detail:** the field list becomes `yamaa`, `type`, `functions`,
  `environments`, `codelists`, `study` and the `sdtm`, `adam` and `send`
  sections; a codelist source is written inline or as a codelist file
  (P-TYPE-1). The last sentence of the statement stays true under D32: the new
  CT card states what a binding without a project file does.
- **Evidence:** `schema-functions` and `sdtm-dm-metadata`, converted in Phase
  2; the Rust test for an unreadable project file follows Q1.
- **Written by:** P1-8.

### PROJECT-2

`rules/profiles/project.md`; changes the title and statement.

Today (title 14 words, statement 68 words):

> `environments` links each host language's environment file, and each host
> reads only its own
>
> `environments` maps a host language key, `python` or `r`, to that language's
> environment file under Project functions. The key states the language of the
> file it names, and an environment file declares no language of its own. Each
> host reads only the entry its own key names; the other entry is neither
> opened nor validated, and one project serves both hosts with one copy of its
> study metadata.

Proposed (title 13 words, statement 64 words):

> `environments` holds each host language's lock, and each host reads only its
> own
>
> `environments.<host>.lock`, under the host language key `python` or `r`,
> names that host's packaging lock, which FUNC-3 verifies; a host without a
> lock needs no entry. Each host reads only the entry its own key names; the
> other entry is neither opened nor validated. Function files are
> language-neutral (FUNC-4), so one project serves both hosts with one copy of
> its study metadata and its functions.

- **Follows:** D8.
- **Fails:** none. Kept until P2-13: `project_environment_missing`.
- **Detail:** "An absent entry leaves that host without an implementation
  environment" becomes: an absent entry means the host declares no lock, and
  its calls run without a package-version audit (FUNC-3); a call in a host
  with no implementation fails under FUNC-1.
- **Evidence:** the Python and Rust tests are rewritten for lock entries.
- **Written by:** P1-8.

### FUNC-1

`rules/profiles/functions.md`; changes the title and statement.

Today (title 16 words, statement 68 words):

> Project functions come only from the environment file the caller's project
> links for the host language
>
> A portable specification may declare a logical `function` call before
> project code exists: structural authoring validates its versionless `name`
> and closed argument-leaf shape, and contract-dependent checks require an
> explicitly supplied project file that links an environment for the host
> language. The project's `environments` entry for that language links the one
> environment file the host reads (PROJECT-2). A specification cannot select,
> replace or override the project or its environments.

Proposed (title 17 words, statement 85 words):

> Project functions come only from the function files the caller's project
> lists, through the running host's implementation
>
> A portable specification may declare a logical `function` call before
> project code exists: structural authoring validates its versionless `name`
> and closed argument-leaf shape, and contract-dependent checks require an
> explicitly supplied project file whose `functions` lists the function file
> defining that name. The host running the build calls the callable that
> file's `implementations` names for its language, and a called function with
> no implementation for that host fails before any input is read. A
> specification cannot select, replace or override the project or its function
> files.

- **Follows:** D8; P-FN-2 (D58).
- **Fails:** `function_implementation_missing`; P1-11 adds
  `project_binding_missing` (D32). Kept until P2-13:
  `project_environment_missing`.
- **Detail:** no function file is discovered, and none is created by omission.
  A call when no project is supplied fails as `project_binding_missing` from
  P1-11, under the new CT card (D71). A call in a host with no implementation
  is reported at each call, with the host language.
- **Evidence:** the three environment tests are rewritten for function files.
- **Written by:** P1-8.

### FUNC-2

`rules/profiles/functions.md`; changes the title and statement.

Today (title 18 words, statement 63 words):

> An environment file is admitted on its own against the environment schema
> before any activation or study read
>
> An environment file is independently admitted against
> `schema_environment.yaml`. `schema_version` selects the schema bundle, and
> there is no environment content version. A missing, unreadable or invalid
> project file, linked environment file or declared metadata resource fails
> before activation or study reads. An invalid project file, environment file,
> definition, test or duplicate logical declaration fails, and static checking
> returns every independently inferable finding before activation.

Proposed (title 18 words, statement 69 words):

> A function file is admitted on its own against the function schema before
> any activation or study read
>
> A function file is independently admitted against the function schema; it
> declares `yamaa` and `type: function` (SCHEMA-2), and there is no function
> content version. A missing, unreadable or invalid project file, listed
> function file or declared metadata resource fails before activation or study
> reads. An invalid definition or test, or a logical name that two listed
> files declare, fails, and static checking returns every independently
> inferable finding before activation.

- **Follows:** D4, D8; P-FN-2 (D58).
- **Fails:** `function_file_invalid`. Kept until P2-13:
  `project_environment_invalid`.
- **Detail:** the paragraphs on environment files describe function files. The
  lock moves to the project file, so its relative path starts at the project
  file (PROJECT-4), and a listed function file that does not exist fails at
  `functions[i]` under FILE-11. Study metadata stays with the project file
  (PROJECT-3). FUNC-11's Fails line follows the same rename to
  `function_file_invalid`.
- **Evidence:** `schema-functions`, whose environment pairs merge into
  function files (D8), and the two Rust tests, rewritten.
- **Written by:** P1-8.

### FUNC-3

`rules/profiles/functions.md`; changes the statement; the title stays.

Today (title 15 words, statement 87 words):

> One host language applies to every function, and a declared lock pins the
> called packages
>
> The `environments` key that links an environment file, exactly `python` or
> `r`, states the language of that file and applies to every function
> definition it declares; an environment file has no language field of its
> own. `lock` optionally names the packaging lock for that language: uv TOML
> for Python or renv JSON for R. On each build that calls a function with a
> declared lock, before any study read, the host verifies the installed
> version of yamaa and of every called function package against the captured
> lock.

Proposed (title unchanged, statement 77 words):

> One host language applies to every function, and a declared lock pins the
> called packages
>
> One host language, `python` or `r`, runs a whole build: each called function
> runs through the callable its `implementations` entry names for that
> language. `environments.<host>.lock` in the project file optionally names
> that language's packaging lock: uv TOML for Python or renv JSON for R. On
> each build that calls a function with a declared lock, before any study
> read, the host verifies the installed version of yamaa and of every called
> function package against the captured lock.

- **Follows:** D8.
- **Fails:** `runtime_artifact_mismatch`, `function_file_invalid`. Kept until
  P2-13: `project_environment_invalid`.
- **Title:** D8 changes the statement only. One language still applies to
  every function of a build, and a declared lock still pins the called
  packages; what changes is where the language and the lock are declared, and
  lock verification is unchanged.
- **Detail:** "yamaa does not search for a lock beside the environment file"
  becomes "beside the project file"; a lock whose format does not match the
  host key that holds it fails as `function_file_invalid`; per-function
  language choices stay invalid, since `implementations` names one callable
  per host. That lock reading moves into Rust or the host standard library
  (D8) is implementation, not card text.
- **Evidence:** `schema-functions` and the two Rust tests, rewritten for
  `environments.<host>.lock`.
- **Written by:** P1-8.

### FUNC-4

`rules/profiles/functions.md`; changes the statement; the title stays.

Today (title 17 words, statement 67 words):

> Each logical function has one closed definition, and a call names only the
> function and its arguments
>
> `functions` maps unique logical names to one definition each, either inline
> or through a path. A definition contains a qualified `function`, a non-empty
> `description`, closed ordered `params`, a scalar `returns`,
> `may_return_missing` (default false), `comparison_decimals` (default four)
> and inline `tests`. A specification call contains only the logical `name`
> and `args`, and its result is one scalar for one logical row. A call naming
> no declared logical function fails.

Proposed (title unchanged, statement 76 words):

> Each logical function has one closed definition, and a call names only the
> function and its arguments
>
> The project's `functions` lists function files, each holding one definition
> under a unique logical `name`. A definition contains `name`, a non-empty
> `description`, closed ordered `params`, a scalar `returns`,
> `implementations` naming a qualified callable for `python`, `r` or both,
> `may_return_missing` (default false), `comparison_decimals` (default four)
> and inline `tests`. A specification call contains only the logical `name`
> and `args`, and its result is one scalar for one logical row. A call naming
> no function the project lists fails.

- **Follows:** D8; P-UNU-1 (D58).
- **Fails:** `unknown_project_function`, as today.
- **Title:** D8 changes the statement only. Each logical function still has
  one closed definition, and a call still names only the function and its
  arguments.
- **Detail:** "A definition file contains that one definition and does not
  repeat its logical name or schema version" becomes: a function file holds
  one definition and declares its own `name`, `yamaa` and `type`; the project
  holds no inline definition (P-UNU-1); `expressions.function.name` selects
  from the function files the project lists.
- **Evidence:** unchanged, converted in Phase 2.
- **Written by:** P1-8.

### JSON-1

`rules/profiles/dataset-json.md`; changes the title and statement.

Today (title 17 words, statement 75 words):

> A section's `datasets` requests one transport file per composed dataset,
> named for the dataset beside the document
>
> The selected project submission section's optional `datasets` requests one
> transport file for each specification in its ordered `specs`: `json` a
> Dataset-JSON file under this profile, and `xpt` a SAS Transport version 5
> file under SAS Transport. Each file is in the directory of the section's
> `define` target, named the lower-case dataset name with the extension
> `.json` or `.xpt`. Omission produces only the Define-XML document. The
> extension mapping of OUTPUT-5 stays closed at `.csv` and `.parquet`.

Proposed (title 17 words, statement 75 words):

> A section's `transport` requests one transport file per composed dataset,
> named for the dataset beside the document
>
> The selected project submission section's optional `transport` requests one
> transport file for each specification in its ordered `specs`: `dataset_json`
> a Dataset-JSON file under this profile, and `xpt` a SAS Transport version 5
> file under SAS Transport. Each file is in the directory of the section's
> `define` target, named the lower-case dataset name with the extension
> `.json` or `.xpt`. Omission produces only the Define-XML document. The
> extension mapping of OUTPUT-5 stays closed at `.csv` and `.parquet`.

- **Follows:** P-PRJ-1 (D58).
- **Fails:** none, as today.
- **Detail and evidence:** `datasets` becomes `transport` and `json` becomes
  `dataset_json`, as in the statement; nothing else changes.
- **Written by:** P1-8.

### XPT-1

`rules/profiles/xpt.md`; changes the title and statement.

Today (title 15 words, statement 63 words):

> A section's `datasets: xpt` writes each composed dataset as one SAS
> Transport version 5 file
>
> `datasets: xpt` requests one transport file per composed specification,
> named and placed as JSON-1 fixes and written by one writer under JSON-2. A
> dataset declared to have no data has no file under JSON-3. The rows are the
> published artifact's rows read as JSON-5 reads them, the dataset's leaf
> names the file under DEFINE-16, and the files publish before the document
> under JSON-14.

Proposed (title 15 words, statement 63 words):

> A section's `transport: xpt` writes each composed dataset as one SAS
> Transport version 5 file
>
> `transport: xpt` requests one transport file per composed specification,
> named and placed as JSON-1 fixes and written by one writer under JSON-2. A
> dataset declared to have no data has no file under JSON-3. The rows are the
> published artifact's rows read as JSON-5 reads them, the dataset's leaf
> names the file under DEFINE-16, and the files publish before the document
> under JSON-14.

- **Follows:** P-PRJ-1 (D58).
- **Fails:** none, as today.
- **Detail and evidence:** unchanged; only `datasets: xpt` becomes
  `transport: xpt`.
- **Written by:** P1-8.

## P1-10: model

### SUB-3

`rules/profiles/submission.md`; changes the title and statement.

Today (title 15 words, statement 67 words):

> A dataset's family is read from the implementation guide the study document
> binds it to
>
> Every dataset follows one foundational standard, and the study document
> binds it: DEFINE-6 resolves each dataset to a declared standard of type
> `IG`. A specification does not name the standard. This profile reads the
> bound standard's published name as one of three closed families, as the
> table below fixes. A dataset bound to an `IG` standard outside the table, or
> to a standard of type `CT`, fails.

Proposed (title 19 words, statement 80 words):

> A specification declares its model, and the implementation guide a document
> binds it to must belong to that model
>
> A specification names the CDISC model its dataset follows in `model`:
> `SDTM`, `SEND` or `ADaM`. `model` is required when the specification
> declares root or column `submission` or a project section lists it, and the
> `sdtm`, `send` and `adam` sections list only specifications of their model.
> When a document composes the specification, the implementation guide
> DEFINE-6 binds it to must be one the table below lists for that model; a
> guide of another model, or a standard of type `CT`, fails.

- **Follows:** D25; P-MODEL-1 (D58).
- **Fails:** `standard_model_mismatch` (Q4), `missing_required_field`. Kept
  until P2-13: `unknown_standard_family`.
- **Detail:** the table's "Family" column becomes "Model", with the values
  `SDTM`, `SEND` and `ADaM`; a declared `submission.class` must belong to the
  model (P-MODEL-1). `model` stays optional elsewhere, since a language test
  follows no model.
- **Evidence:** `sdtm-dm-metadata`, converted with `model: SDTM`, and new
  negatives for an absent required `model` and for a specification in another
  model's section, authored independently. P1-10 measures whether any
  negative's first issue changes (plan question 5).
- **Written by:** P1-10.

### SUB-23

`rules/profiles/submission.md`; changes the title and statement.

Today (title 25 words, statement 55 words):

> Rules needing no family or project file, and the `mandatory` resolution, are
> checked on the specification alone, and the rest when a document composes it
>
> Every rule of this profile that needs neither a family nor the project file
> is checked when the specification is validated on its own, and so is the
> resolution and enforcement of `mandatory`, which reads its family group from
> the dataset's `class`. Every other rule is checked when Define-XML composes
> the specification into a document.

Proposed (title 19 words, statement 85 words):

> Rules needing no project file are checked on the specification alone, and
> the rest when a document composes it
>
> Every rule of this profile that needs no project file is checked when the
> specification is validated on its own, including each rule that depends on
> the model the specification declares (SUB-3), such as core designations,
> origin pairs and date types, and the resolution of `mandatory`. A rule that
> reads the project file, such as the guide a section binds, the documents a
> section declares or the labels of a predecessor another listed specification
> declares, is checked when Define-XML composes the specification into a
> document.

- **Follows:** D25; P-MODEL-1 (D58).
- **Fails:** none, as today.
- **Detail:** the specification-time list gains SUB-10's core designations and
  `core`/`mandatory` conflicts, the class's agreement with the model, the date
  types of SUB-6 and SUB-25, SUB-15's origin pairs, and SUB-5's and SUB-9's
  prohibitions (P-MODEL-1). The composition list keeps the bound guide,
  SUB-17's references and SUB-20's labels. The closing paragraph says that the
  specification names its model and the document its standard.
- **Evidence:** unchanged; `sdtm-lb-metadata` runs with its
  `submission-project.yaml` under D32 (plan question 4).
- **Written by:** P1-10.

## P1-12: copy

### New SPEC card: `copy`

Proposed (title 20 words, statement 87 words):

> A `copy` entry declares the listed columns of a schema-linked input under
> their names, with the producer's type and label
>
> A `columns` entry `copy: {dataset: D, columns: [...]}` declares one column
> per listed name, and each name must be an output column of the producer that
> input `D` links through `schema` (INPUT-10). Each copied column reads the
> same-named column of `D` unchanged and takes the producer's `type`, `label`
> and content metadata; its submission origin is `Predecessor`, and an
> optional `submission` adds only `core`, `mandatory` and `role`. The entry
> expands to ordinary columns in the resolved specification, and a copied
> column is published only when `output.columns` lists it.

- **Follows:** D19, D70; P-COPY-1 (D58).
- **Fails:** no condition is approved for `copy`; recommended reuse (Q5):
  `unknown_field`, `duplicate_identifier`, `prohibited_construct`.
- **File and number:** `rules/specification.md`, topic Columns, after SPEC-7,
  as number 9 of the SPEC series, the next free number on main (Q5).
- **Detail:** the producer's content metadata is `data_type`, `length`,
  `significant_digits`, `display_format` and `codelist` where it declares
  them, except that a `date` or `datetime` column is submitted as the
  consumer's model requires (SUB-6, SUB-25). The origin's description is
  `<producer domain>.<column>`. `comment`, `method`, `value_level` and
  `origins` are not allowed in the entry. A copy from an input that does not
  drive the rows is an implicit join and needs the input's `key` (LOOKUP-1). A
  later layer overrides a copied column by name, and `clear:` addresses it
  (LAYER-5, LAYER-9). A renamed copy is an ordinary column (SUB-20).
- **Evidence:** new benchmarks, authored independently: a copy from a linked
  ADSL, and a negative for each failure.
- **Written by:** P1-12.

### SUB-20

`rules/profiles/submission.md`; changes the title and statement.

Today (title 14 words, statement 76 words):

> A Predecessor column is a bare `source` copy of the column its description
> names
>
> A column whose `origin.type` is `Predecessor` copies its values unchanged
> from the predecessor dataset and column `origin.description` names, so its
> derivation must be a bare `source` of that dataset and column, declaring a
> `filter` when the description writes a where clause. When the column's name
> equals the predecessor column's name and a specification some submission
> section of the project file lists declares that column, their labels must be
> equal; a renamed column keeps its own label.

Proposed (title 13 words, statement 89 words):

> A Predecessor column is a bare copy of the column its description names
>
> A column with a `Predecessor` origin copies its values unchanged from the
> predecessor dataset and column the origin's `description` names, so its
> derivation must read that dataset and column as a bare reference, or as a
> structured `source` declaring a `filter` when the description writes a where
> clause; a `copy` entry declares such columns. When the column's name equals
> the predecessor column's name and a specification some submission section of
> the project file lists declares that column, their labels must be equal; a
> renamed column keeps its own label.

- **Follows:** D16, D18, D19; P-ORIGIN-1 and P-COPY-1 (D58).
- **Fails:** `origin_contradicts_derivation`, `refuted_value_origin`,
  `predecessor_label_mismatch`, as today.
- **Title:** "bare `source` copy" becomes "bare copy": in 1.0 a bare reference
  is the spelling of a copy (D18), and a `copy` entry also makes one.
- **Detail:** `origin.description` becomes the origin's `description`, and a
  `Predecessor` origin stands alone (SUB-14). A copy whose input identifier
  differs from its producer's `domain` gets a description naming the
  producer's domain while it reads through the input's identifier; P1-12
  settles in detail whether this card then reads through the input linked to
  the predecessor or the copy fails.
- **Evidence:** unchanged, plus the copy benchmarks.
- **Written by:** P1-12.

## P1-13: origins and standards

### SUB-14

`rules/profiles/submission.md`; changes the statement; the title stays.

Today (title 14 words, statement 52 words):

> Every column with submission metadata declares an origin from closed type
> and source vocabularies
>
> `origin` is required for every column carrying submission metadata.
> `origin.type` is one of `Assigned`, `Collected`, `Derived`, `Not Available`,
> `Other`, `Predecessor` or `Protocol`. `origin.source` is one of
> `Investigator`, `Sponsor`, `Subject` or `Vendor`. Both vocabularies are
> closed, and neither is extended by a specification. An `origin` absent from
> a column carrying submission metadata fails.

Proposed (title unchanged, statement 78 words):

> Every column with submission metadata declares an origin from closed type
> and source vocabularies
>
> `origins` is required for every column carrying submission metadata and
> lists one or more origins. Each origin's `type` is one of `Assigned`,
> `Collected`, `Derived`, `Not Available`, `Other`, `Predecessor` or
> `Protocol`, and its `source` one of `Investigator`, `Sponsor`, `Subject` or
> `Vendor`. Both vocabularies are closed, and neither is extended by a
> specification. Two origins with the same type and source, or a `Predecessor`
> or `Not Available` origin beside another, fail, as does a column carrying
> submission metadata without `origins`.

- **Follows:** D16, D70; P-ORIGIN-1 (D58); the condition `conflicting_origins`
  (D69).
- **Fails:** `origin_missing`, `conflicting_origins`.
- **Title:** it stays, since "an origin" admits several.
- **Detail:** an empty `origins` fails as `origin_missing`; `Not Available` is
  refused for SDTM and ADaM by SUB-15's pairs (D65), not here; a value-level
  entry takes `origins` too (SUB-13).
- **Evidence:** `sdtm-dm-metadata`, converted to one-entry lists, and a new
  positive with two origins and a negative for `conflicting_origins`, authored
  independently.
- **Written by:** P1-13.

### DEFINE-8

`rules/profiles/define-xml.md`; changes the statement; the title stays.

Today (title 12 words, statement 50 words):

> Composition resolves the codelist, standard and document names a
> specification leaves unresolved
>
> Composition resolves the names Submission metadata and Controlled
> terminology leave unresolved: a column's `codelist` and every document
> reference. A name matching no declaration fails. Resolution also determines
> each dataset's family from the standard DEFINE-6 binds and the table of
> SUB-3, and every family-dependent rule of Submission metadata is applied
> here.

Proposed (title unchanged, statement 59 words):

> Composition resolves the codelist, standard and document names a
> specification leaves unresolved
>
> Composition resolves the names Submission metadata and Controlled
> terminology leave unresolved: a column's `codelist`, every document
> reference, and the standard a dataset's `submission.standard` selects from
> the section's `standards`, the first entry when it is omitted. A name
> matching no declaration fails. Composition also checks that the selected
> standard is an implementation guide of the model the specification declares
> (SUB-3).

- **Follows:** D16, D25; P-STD-1 and P-MODEL-1 (D58).
- **Fails:** `unknown_standard`, `unknown_codelist`, `unknown_document`, as
  today.
- **Detail:** the name that fails as `unknown_standard` is
  `submission.standard`, which D16 makes reachable (P-STD-1, finding F-5);
  names are unique within a section, so OIDs stay `STD.<name>`; the rules that
  depended on the family move to specification time (SUB-23).
- **Evidence:** a new negative for an unknown standard, authored
  independently.
- **Written by:** P1-13.

### DEFINE-19

`rules/profiles/define-xml.md`; changes the title and statement.

Today (title 10 words, statement 57 words):

> Each column produces one base `ItemDef` carrying exactly one `def:Origin`
>
> Each column produces one base `ItemDef` with the attributes and children the
> table below lists; per-value definitions use the same mapping with the
> effective metadata of DEFINE-27. `def:Origin` carries `Type` and, when
> Submission metadata admits one, `Source`. Exactly one `def:Origin` is
> generated per base or per-value `ItemDef`. A `def:DocumentRef` carries
> `leafID`, the referenced document's generated leaf identifier.

Proposed (title 12 words, statement 60 words):

> Each column produces one base `ItemDef` carrying one `def:Origin` per
> declared origin
>
> Each column produces one base `ItemDef` with the attributes and children the
> table below lists; per-value definitions use the same mapping with the
> effective metadata of DEFINE-27. A base or per-value `ItemDef` carries one
> `def:Origin` per origin, in declaration order, each with `Type` and, when
> Submission metadata admits one, `Source`. A `def:DocumentRef` carries
> `leafID`, the referenced document's generated leaf identifier.

- **Follows:** D16, D70; P-ORIGIN-1 (D58).
- **Fails:** none, as today.
- **Detail:** `origin.description` becomes each origin's `description`, and
  each origin's document references follow it; a value-level entry without
  origins of its own takes the column's (DEFINE-27).
- **Evidence:** `sdtm-dm-metadata` and `sdtm-lb-metadata` keep their bytes,
  one origin each; a new define benchmark with two origins, authored
  independently.
- **Written by:** P1-13.

## P1-15: ODM input

### INPUT-1

`rules/inputs.md`; changes the statement; the title stays.

Today (title 14 words, statement 66 words):

> The input path's extension selects the profile that reads it, and nothing is
> sniffed
>
> `dataset_class.path` names the file a specification reads, and its
> case-insensitive extension selects the source profile that reads it: `.csv`,
> `.parquet`, `.sas7bdat` or `.xpt`, including for an input linked to its
> producing specification. No reader inspects a file's contents to choose a
> profile, a delimiter or a quote character, and no unknown extension falls
> back to a default: it fails during validation, before any byte is read.

Proposed (title unchanged, statement 81 words):

> The input path's extension selects the profile that reads it, and nothing is
> sniffed
>
> `dataset_class.path` names the file a specification reads, and its
> case-insensitive extension selects the source profile that reads it,
> including for an input linked to its producing specification: `.csv`,
> `.parquet`, `.sas7bdat`, `.xpt`, or `.xml` for an ODM 1.3 or 2.0 document
> read into the item table of NAME-8. No reader inspects a file's contents to
> choose a profile, a delimiter or a quote character, and no unknown extension
> falls back to a default: it fails during validation, before any byte is
> read.

- **Follows:** D26; P-ODMIN-1 (D58); D73.
- **Fails:** `source_profile_unknown`, as today.
- **Detail:** the table gains `.xml`, profile `odm`, ODM 1.3 or 2.0 XML,
  read-only, owned by a card for the ODM source profile that the plan does not
  list (last section); `read_odm` applies the same refusals (D73).
- **Evidence:** no benchmark reads ODM XML today; a positive and the two
  refusals, authored independently.
- **Written by:** P1-15, outside the P1-3 to P1-13 range #3162 names.

## Cards the plan did not list

The plan's list took the titles that name 0.5 spellings and the statements
the RFC names. Checking every card's title and statement against the approved
rename table and decisions finds more cards whose text 1.0 changes. Each needs
the maintainer's approval as much as the cards above (`rules/README.md`).

**Substitutions.** In each, a decided spelling or term replaces the 0.5 one,
and nothing else changes. D71 approved one-line changes of this kind for
LOOKUP-2 and CT-1 on the [rules page](rules.md). **Recommendation:** approve
these as written.

| Card | Changes | Change | Follows | Pull request |
| --- | --- | --- | --- | --- |
| SPEC-4 | statement | "it is written `literal: null`" becomes "it is written `derivation: null`" | D18, P-NULL-1 | P1-5 |
| EXPR-6 | statement | "`source` returns the value of its variable and `literal` its YAML scalar" becomes "a reference or a structured `source` returns the value of its variable, and a literal its value" | D18 | P1-5 |
| EXPR-8 | statement | "be `{literal: null}` for missing" becomes "be YAML null for missing" | D18 | P1-5 |
| COLUMN-12 | statement | "the concise `source: DATASET.VARIABLE` form" becomes "a bare reference such as `DATASET.VARIABLE`" | D18 | P1-5 |
| SUB-19 | statement | "a `source`" becomes "a reference or a structured `source`", and "A bare `literal`" becomes "A literal" | D18 | P1-5 |
| TIME-1 | statement | "never enters `compute` arithmetic" becomes "never enters arithmetic" | D18 | P1-5 |
| VALUE-18 | title, statement | "compute arithmetic" becomes "arithmetic" | D18 | P1-5 |
| EXPR-13 | statement | "With `right: true`" becomes "With `closed: right`", and "with `right: false`, the default," becomes "with `closed: left`, the default," | P-ORD-2 | P1-5 |
| TIME-5 | statement | `bounds` becomes `closed`, `exclusive` becomes `right`, `inclusive` `both` and `between` `neither`; `diff` takes no `left`, as the rename table maps none; D71 approved only the title | P-ORD-2 | P1-5 |
| TIME-6 | statement | "of `date` against `reference`" becomes "of `source` against `reference`" | P-OP-1 | P1-5 |
| TIME-8 | statement | "`minimum_source_precision`, default `year`, is the coarsest collected precision completed; a coarser source gives missing" becomes "A source missing a field that no declared rule supplies gives missing" | D61, P-IMP-1 | P1-5 |
| WINDOW-9 | statement | `value` becomes `source` and `reference_date` becomes `reference`; "ordered by `date` and then by the `tie_break` terms" becomes "ordered by `window.order_by`", and "tied for last on `date` and every term" becomes "tied for last on every term" (Q3); D71 approved only the title | P-OP-2, P-OP-3, P-ORD-1 | P1-5 |
| ROW-2 | statement | `no_match` becomes `absent` | P-HND-1 | P1-5 |
| EXPR-14 | statement | "derive bindings" becomes "aggregate derivations", and "inferred keys" goes | D23, P-BIND-1 | P1-5 |
| VALUE-24 | statement | "aggregate derive bindings" becomes "aggregate derivations" | P-BIND-1 | P1-5 |
| LOOKUP-4 | statement | "with or without `no_match: {literal: null}`" becomes "with or without `absent: null`", beside the key comparison D71 described | D18, P-HND-1 | P1-6 |
| LOOKUP-14 | statement | "whether the pairs were inferred or declared" becomes "whether the lookup or its input declares the pairs" | D23, P-KEY-1 | P1-6 |
| LOOKUP-19 | statement | "the applicable keys it joins on" becomes "the key it joins on" | D23 | P1-6 |
| PROJECT-3 | statement | the section's `standard` becomes its `standards`, and "of that family" becomes "of that model" | D16, P-MODEL-1 | P1-8 |
| PROJECT-4 | title, statement | the title becomes "A project file's relative paths resolve from the project file"; the statement lists each function file, lock, codelist source, section specification and Define-XML target, and loses its sentence on environment files | D8 | P1-8 |
| PROJECT-13 | statement | "declares `datasets`" becomes "declares `transport`" | P-PRJ-1 | P1-8 |
| JSON-12 | statement | "sets `datasets: json`" becomes "sets `transport: dataset_json`" | P-PRJ-1 | P1-8 |
| DEFINE-2, DEFINE-16 | statement | "declares `datasets`" becomes "declares `transport`" | P-PRJ-1 | P1-8 |
| DEFINE-22, XPT-5 | statement | "`datasets: xpt`" becomes "`transport: xpt`" | P-PRJ-1 | P1-8 |
| XPT-9 | statement | "`datasets: xpt`" becomes "`transport: xpt`", and "An `adam` date" becomes "An `ADaM` date" | P-PRJ-1, P-MODEL-1 | P1-8 |
| DEFINE-26 | statement | "`schema_project.yaml`" becomes "the project schema" | D4 | P1-8 |
| DEFINE-28 | title, statement | the section's `standard` becomes `standards`, `sdtm.standard` becomes `sdtm.standards`, and "the selected family" becomes "the selected model" | D16, P-MODEL-1 | P1-8 |
| FILE-1 | statement | "which links the implementation environment file of each host language" becomes "which lists the study's function files", and the environment file leaves the list of files whose paths resolve from themselves | D8 | P1-8 |
| SPEC-8 | statement | "project files and environment files" becomes "project files, codelist files and function files" | D8, P-TYPE-1 | P1-8 |
| FUNC-5, FUNC-7 | statement | "an environment `default`" and "the environment default" become "a `default`" and "the default" | D8 | P1-8 |
| FUNC-8 | statement | "`function` is a statically written fully qualified callable" becomes "each `implementations` entry is a statically written fully qualified callable", and "environment defaults" becomes "defaults" | D8 | P1-8 |
| SUB-5, SUB-6, SUB-9, SUB-10, SUB-15, SUB-25, SUB-27, DEFINE-15, XPT-6 | every statement, and every title but those of SUB-9 and SUB-10 | "family" becomes "model", and the family names `sdtm`, `send` and `adam` become the models `SDTM`, `SEND` and `ADaM`; SUB-10's title is D71's | P-MODEL-1 | P1-10 |
| SUB-13 | statement | "An entry's own origin is refuted" becomes "An entry's own origins are refuted" | P-ORIGIN-1 | P1-13 |
| SUB-16 | statement | "`origin.description`" becomes "An origin's `description`", and "when `type` is" becomes "when its `type` is" | P-ORIGIN-1 | P1-13 |
| SUB-18 | statement | "A column whose `origin.type` is `Derived`" becomes "A column with a `Derived` origin" | P-ORIGIN-1 | P1-13 |

**New wording.** These need text that this page does not draft.

| Card | Changes | What changes | Follows | Pull request |
| --- | --- | --- | --- | --- |
| SCHEMA-1 | statement | names the 1.0 schema files of the four file types instead of `schema_project.yaml` and `schema_environment.yaml`, once P1-1 fixes their names | D4, D8, P-TYPE-1 | P1-3, or P2-13 when `yaml/v1/` moves up |
| FORMULA-1, FORMULA-9, FORMULA-12 | statement | `compute.expr` becomes the derivation field of FORMULA-27 | D18 | P1-2 |
| FORMULA-27 | statement | the derivation field's constructs, which the RFC describes without text | D18 | P1-2 |
| COLUMN-11 | title, statement | `absent` answers only a lookup with no record, and a variable that does not exist fails (NAME-3) | D67, P-HND-1 | P1-5 |
| EXPR-3 | statement | the leaves become grammar text and the structured `source` binding, and `literal` goes | D18 | P1-5 |
| EXPR-4 | title, statement | `literal` goes: a reference copies a named variable, and a literal returns its value unchanged | D18 | P1-5 |
| DEFINE-6 | title, statement | each dataset is bound to the standard its `submission.standard` selects, the section's first by default, instead of one guide for every dataset of the section | D16, P-STD-1 | P1-13 |
| SUB-12 | statement | a where column also takes `{comparator: NE, values: [...]}` or `NOTIN`, which selects a row whose value is not listed, missing included | D66, P-VLM-1 | P1-14 |
| NAME-8 | statement | an `.xml` input is read into the eleven fields by the ODM profile, so the header and type checks apply to CSV and Parquet only | D26 | P1-15 |
| New ODM card | new card | the ODM source profile: snapshots read, transactional content refused as `source_odm_transactional`, a file without clinical data refused as `source_odm_no_clinical_data`, and audit records and signatures skipped | D26, D73, P-ODMIN-1 | P1-15 |
| CSV-10 | title, statement | a declared byte-order mark is admitted; it has no spelling yet (#3157), and the plan schedules no pull request for it | D15 | none yet |

**Recommendation:** draft these on a follow-up page before the pull request
that writes each, starting with the P1-5 cards; SCHEMA-1 waits for P1-1's file
names, and CSV-10 for #3157.

Two more changes D71 described without drafting them, and approved in that
form: CT-1's statement gains the case without a project file (P1-11), and
LOOKUP-2's "the applicable keys" becomes "the input's declared key" (P1-6).
`rules/README.md` is not a card, but its Terminology entry for the project file
and the environment file, and its Rule files rows for the two profiles, change
with P1-8.

## Approval

Please approve this page on #3030, as a whole or item by item: by card ID, by
question (Q1 to Q6) and by row of the substitution table. Approval covers each
proposed title and statement and each substitution; the cards that need new
wording come back on a follow-up page.
