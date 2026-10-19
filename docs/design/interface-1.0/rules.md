# Rules: contradictions, #3048 tightening and proposed card text

Part of the [1.0 interface RFC](../interface-1.0.md). This page gives the
disposition of each rule contradiction of #3030 Phase 0 item 6, the rules
items of #3048, the requirement-to-rule rewrite of D28, and the proposed text
of every card the RFC would change. Nothing here changes a card: Phase 1
writes the cards. The owner approved the proposed text on 2026-10-19 (#3030
D71). The text that 1.0 needs beyond it is proposed on
[Card text beyond D71](card-text.md).

`rules/README.md` sets the approval level. A change to a card's title or
statement changes the language and needs the maintainer's approval; a change
to detail, evidence or tests needs ordinary review. Every proposed title below
is one sentence without a final period, and every statement has at most 90
words, counted as `tools/rules/model.py` counts them. Rule IDs are written
without links here; the cards link each one. A new card is named by its file
and topic, since its number is the next free one when it is written
(`rules/README.md`: numbers are never reused).

## Contradictions (Phase 0 item 6)

### P-RULE-2. JSON-15 against INPUT-1

JSON-15 (`rules/profiles/dataset-json.md`) says a Dataset-JSON file is never
read as a source, and that the input extension mapping "stays closed at
`.csv` and `.parquet`". INPUT-1 maps `.csv`, `.parquet`, `.sas7bdat` and
`.xpt`, and D26 adds the ODM profile next. JSON-15 restates a list that went
stale when the SAS inputs arrived.

| Option | Change |
| --- | --- |
| **a** | **Rewrite JSON-15 to cite INPUT-1 without restating its list** |
| b | Delete JSON-15 and add one sentence to INPUT-1's detail |
| c | Keep |

Proposed JSON-15 statement (38 words), a statement change:

> Dataset-JSON is a transport profile that only a project submission writes
> (JSON-1). `.json` names no source profile under INPUT-1, so an input whose
> `path` ends in `.json` fails as `source_profile_unknown` during validation,
> as any other unmapped extension does.

Add `Fails: source_profile_unknown.` and, as evidence,
`negative-source-unknown-format` until a `.json` negative exists. INPUT-1 then
owns the one list, and D26 changes one card.

### P-RULE-3. CHECK-14's nonexistent `unmatched: fail`

CHECK-14's detail says a group absent from the artifact is rejected by "a
record lookup declaring `unmatched: fail`". No field named `unmatched` exists;
the mechanism is LOOKUP-24's `require_match: true`, which fails as
`unmatched_key`. A detail change, for ordinary review: "a lookup declaring
`require_match: true` (LOOKUP-24) fails as `unmatched_key` when the data
cannot supply an expected group." P-HND-2 keeps `require_match`, so the
sentence holds after the rename table.

### P-RULE-4. SPEC-6's undefined "uses a lookup"

SPEC-6 makes a column-level derivation row-local "unless it uses a lookup, an
aggregate or a window". LOOKUP-25 makes every implicit join, named
intermediate and qualified aggregate a lookup, so read literally every
`ADSL.AGE` read is column-phase, which SPEC-6's own detail contradicts. The
code (`default_reads` in `rust/crates/yamaa-core/src/specification/rows.rs`)
treats a default as not row-local only for an aggregate, a window operation
(also inside `case`), a read of a `SELF` intermediate, or a read of a column
that is not row-local. The style guide's `repeated_row_derivation` uses a
third definition, in which reading any named intermediate makes a derivation
not row-local.

Proposed SPEC-6 statement (90 words), restating what the code decides:

> A column-level derivation is row-local unless it holds an aggregate or a
> window operation, reads a `SELF` intermediate, or reads a column whose
> column-level derivation is not row-local; a read through the implicit join
> or of an input-backed named intermediate is row-local. A row-local
> column-level derivation is its column's default for every `rows` entry that
> names the column or whose row-phase context reads it, and an entry's own
> derivation overrides it for that entry's rows. Another column-level
> derivation keeps its column-phase meaning, and a `rows` entry naming its
> column fails.

Follow-ups: rename the context value `named_intermediate` to
`self_intermediate`, since it means only a `SELF` read; `lint`'s
`repeated_row_derivation` uses SPEC-6's definition when it moves to Rust
(D40); "row-phase" and "column-phase" change with P-PHASE-1.

### P-RULE-5. Conditions no card cites

Of 202 registered conditions, six appear on no card's Fails line. Each is
raised by causes whose REQ citation maps to a card that states the failure in
prose, so each joins that card's Fails line, a metadata change:

| Condition | Phases | Raised by | Card | Negative benchmarks |
| --- | --- | --- | --- | --- |
| `aggregate_multiple_records` | derivation, row_construction | `AggregateMultipleRecords`, `AggregateRowMultipleRecords` (REQ-0501) | FORMULA-20 | `negative-adlb-duplicate-wbc`, `negative-multiple-baselines` |
| `correlated_filter_with_unique_verification` | validation | `IntermediateUniqueCorrelatedFilter` (REQ-1245) | LOOKUP-17 | none |
| `mixed_relations` | validation | `AggregateMixedRelations` (REQ-0504) | LOOKUP-18 | none |
| `source_system_incomplete` | validation | `SubmissionSourceVersion` (REQ-0979) | DEFINE-12, with a detail sentence | none |
| `unknown_derive_variable` | validation | `AggregateDeriveVariable` (REQ-1189) | LOOKUP-22 | none |
| `zero_offset` | validation | `WindowZeroOffset` (REQ-0328) | WINDOW-7 | `negative-row-no-prior` |

Two more sit outside the rule:

- `unknown_standard` is registered and on DEFINE-8's Fails line, but no cause
  raises it. D16 makes it reachable (a `submission.standard` naming no entry
  of the section's `standards`, P-STD-1): wire it in Phase 3.
- `cancelled`, in the unregistered phase `check`, is built as an issue row in
  `rust/crates/yamaa/src/internal.rs` (line 504). `check_diagnostics.py` scans
  only for registered names, so it misses the row. Remove it (finding F-1): an
  interrupt raises the host's interrupt (D46), and a Rust embedder gets
  `Error::Cancelled`. Registering `cancelled` at phase `engine` is the
  alternative. (The two other rows built outside the registry, `conversion_probe`
  and `finding` in phase `input`, are unit-test fixtures.)

### P-PHASE-1. Stage against phase

The registered condition phases, with the conditions that may report each,
the core registry causes that do, and the negative benchmarks that expect
each:

| Phase | Conditions | Causes | Negatives |
| --- | --- | --- | --- |
| `validation` | 146 | 299 | 120 |
| `ingest` | 17 | 19 | 8 |
| `verification` | 13 | 16 | 21 |
| `derivation` | 12 | 12 | 11 |
| `output` | 8 | 8 | 5 |
| `impute` | 4 | 6 | 7 |
| `mapping` | 3 | 3 | 1 |
| `row_construction` | 2 | 2 | 1 |
| `join` | 2 | 2 | 11 |
| `convert` | 1 | 5 | 4 |
| `engine` | 1 | 1 | 0 |
| `planning` | 1 | 1 | 0 |

Beside them the repository uses: COLUMN-10's handler table, with a "Stage"
column of 9 names, four of them not phases (`bind`, `cut`, `extract`,
`template`); "stage" for a condition phase 6 times; "stage" for the four
steps of a value's lifecycle (COLUMN-8) 13 times; "row phase" and "column
phase" for derivation timing 19 times; the `stage` context key of
`engine_rejected`, with 14 values; and the facade's progress enums.

| Option | Change |
| --- | --- |
| **A** | **`phase` is the 12 registered values, unchanged and defined by a card; "stage" only for the value lifecycle** |
| B | Register the four handler stages as phases, which changes `phase` in issues and expected files |
| C | Keep both and document them, which fails 2.7's "one vocabulary" |

**New COLUMN card** (title 19 words, statement 68 words):

> Every condition reports one phase from a closed list, which names the part
> of the run that decides it
>
> A condition reports one phase: `validation`, `planning`, `ingest`,
> `row_construction`, `join`, `mapping`, `impute`, `derivation`, `convert`,
> `verification`, `output` or `engine`, as the table below defines.
> `yaml/conditions.yaml` registers the phases each condition may report, and
> reporting any other is a defect. The numbered stages of a value (COLUMN-8)
> are not phases: stage 1 reports the phase of what it evaluates, stages 2
> and 3 report `convert`, and stage 4 reports `verification`.

| Phase | Decided while |
| --- | --- |
| `validation` | checking files before any input byte is read (P9) |
| `planning` | lowering a valid specification to a plan (`unsupported_operation`) |
| `ingest` | reading an input snapshot into typed records |
| `row_construction` | building output rows (ROW) |
| `join` | a lookup's match, narrow and select steps (LOOKUP-25) |
| `mapping` | the value-mapping verbs `mapping`, `cut`, `str_extract` and `str_template` |
| `impute` | completing collected dates and times (TIME) |
| `derivation` | evaluating any other expression of a value |
| `convert` | converting a completed result to its column type, and handling a failed conversion (COLUMN-8, stages 2 and 3) |
| `verification` | column and dataset verifications (CHECK) |
| `output` | validating keys, rendering and publishing (OUTPUT) |
| `engine` | an application refusal outside the language (`engine_rejected`) |

Editorial changes with it: COLUMN-10's "Stage" column becomes "Phase", with
`validation` for `bind` and `mapping` for `cut`, `extract` and `template`;
the six "`<phase>` stage" phrases become "`<phase>` phase"; "row phase" and
"column phase" become "row construction" and "column derivation" (the
condition `phase_boundary` keeps its name); `engine_rejected`'s context key
`stage` becomes `step`. Optionally, before the Rust inventory freezes, the
progress enums become `Step`, `DatasetStep` and `CallStep`.

### P-JOIN-1. P1 against LOOKUP-1 (D23)

P1 says nothing is inferred from a same-named field. LOOKUP-1 joins on "the
applicable keys", the output keys the input also carries; LOOKUP-3 and
LOOKUP-5 infer the same for a named intermediate or a qualified aggregate
that omits `key`. #3030 also cites a LOOKUP card numbered 21, which #3021
retired when it ported the one lookup model; its rule for an aggregate's keys
now lives in LOOKUP-5 and LOOKUP-25.

Disposition (D23, with D23-R1 and P-KEY-1): P1 stands; "applicable keys"
leaves the rules; the converter writes today's inferred keys, so no
benchmark's bytes change. Proposed text:

**LOOKUP-1** (title 13 words, statement 88 words):

> A dataset-qualified source reads through the implicit join on the input's
> declared key
>
> A scalar source qualified by an input reads it through the implicit join, a
> lookup whose key pairs are that input's declared `key` columns, each
> matched against the current row's column of the same name. A structured
> `source:` binding adds its `between`, `filter`, `order_by`, `keep`, `absent`
> and `require_match` to the join (LOOKUP-25). The join gives one value per
> current row and preserves the row. A qualified read of an input that
> declares no `key` fails validation, and the qualifier of the row template's
> driver is not a join.

**LOOKUP-3** (title 11 words, statement 57 words):

> A named intermediate declares an id, a dataset and a key
>
> An entry of `intermediates` declares `id`, `dataset`, the relation it
> reads, and `key`, its match (LOOKUP-5), unless it drives a row template,
> which declares none (ROW-2). Its `id` shares one naming list with dataset
> identifiers, other lookup ids and the output `domain`, and a collision
> fails. An intermediate read as a lookup without `key` fails as
> `missing_required_field`.

**LOOKUP-5** (title 14 words, statement 89 words):

> A key pairs each column of the lookup's relation with a current-row match
> value
>
> `key` states the match as pairs, each a column of the lookup's relation and
> the current-row match value it must equal. A list of column names pairs
> each with the same-named current-row value; a mapping pairs each column,
> written as its key, with a variable or expression. A named intermediate
> states its own `key`; an implicit join, and a qualified aggregate without
> `key`, use the input's declared `key`. A lookup names at least one pair,
> every column must exist, and every match value must read a known
> current-row value.

LOOKUP-2 replaces "the applicable keys" with "the input's declared key";
LOOKUP-4's `rename_only_intermediate` compares an intermediate's key with its
input's declared key; LOOKUP-25's "Match keys" row reads "the input's `key`",
"`key`" and "`key`, or the input's `key`"; `no_applicable_keys` is raised for
a qualified read of an input that declares no key, at the read, naming the
input. `yamaa.style`'s `redundant_intermediate` compares a written key with
the input's declared key when it moves to Rust.

### P-RULE-7. CT-9 and CT-11 (D32)

CT-9's detail says a build without a project "does not read the binding", so
a spec checked alone passes while the same spec fails in its project. CT-11's
detail says a build "admits no name without" a project, so alone every
present value fails at verification time, after data is read. CT-1's
`unknown_codelist` applies only where a project supplies codelists. One spec
thus validates differently alone and in its project, against I10.

| Option | Condition |
| --- | --- |
| **a** | **One new condition, `project_binding_missing`, for every binding checked without a project file; a project that lacks the named item keeps that item's condition** |
| b | Reuse the per-kind conditions (`unknown_codelist`, the D8 function condition) and add one for the inventory |

Option a follows "one fault keeps one condition": the two fixes differ (pass
a project, or declare the item). **New CT card** (title 17 words, statement
64 words):

> A project binding needs a project file, and a specification checked or run
> without one fails validation
>
> A specification that binds a codelist, declares an `inventory_vocabulary`
> or calls a project function names something only a project file supplies.
> Checked or run without a project file, it fails validation with
> `project_binding_missing` at each such binding, before any input is read.
> With a project file, a binding to an item the project does not declare
> fails as CT-1 and the project-function rules state.

CT-1's statement adds the case without a project; CT-9's detail drops its
last paragraph; CT-11's detail replaces "admits no name without one" with the
validation failure; FUNC-1's case without a project changes condition.
Evidence: new negative benchmarks for a spec run alone with a codelist
binding and with an inventory vocabulary, authored independently.

### P-RULE-8. COLUMN-5 against LAYER-13 (D33)

COLUMN-5 orders columns by declaration and re-sorts only a spec that declares
a `parent`; LAYER-13 states dependency order without that scope, so it reads
as if every spec were re-sorted. D33 keeps both behaviors and documents the
asymmetry. **LAYER-13** (title 12 words, statement 65 words):

> A layered specification's resolved columns follow dependency order with a
> stable tie-break
>
> When a specification declares a `parent`, each resolved column comes after
> every column its derivation reads, after pruning; when more than one column
> can come next, the one with the earliest contribution position comes first,
> which keeps an ancestor's independent columns before its descendants'. A
> specification without a `parent` keeps its declaration order, which COLUMN-5
> requires to be a dependency order. `output.columns` is never reordered.

The schema introduction gains one paragraph: a single file keeps the order it
is written in, which must be a dependency order, while a layered
specification is re-sorted, because a child layer cannot insert a column
between its parent's.

### P-RULE-9. Failures without a condition

PROJECT-12 and PROJECT-13 say that saving a run or a submission that built
nothing "fails", with no registered condition, against `rules/README.md` ("A
failure needs a registered condition named in `Fails`"). The facade refuses
those cases with text only, as it does an unknown member name and an
internal resolution defect. Proposed: `nothing_to_save` (phase `output`) on
both cards' Fails lines; the member lookup refuses with
`unknown_specification_name` (PROJECT-6); "invalid resolved facts" becomes
`engine_rejected`. Every refusal then carries an issue, which `status` and
D39's error text need.

## Requirement becomes rule (P-RULE-1, D28)

The codemod #3048 mentions is `tools/rules/migrate.py`, written for the 0.5.0
card conversion. Its `links` subcommand rewrites markdown links to replaced
anchors and bare replaced IDs under `rules/`; its docstring says that Rust,
YAML and other citations "keep resolving through `Replaces`". It maps every
citation through `RuleSet.replaced`, but rewrites only markdown.

| Measure | Count |
| --- | --- |
| Cards; with a `Replaces` line; replaced IDs | 402; 376; 1,234 |
| REQ occurrences in authored files | 4,703 in 501 files: 1,557 `.rs`, 1,349 `.md` (1,234 of them `Replaces` lines), 903 `.json`, 278 `.py`, 267 `.tsv`, 258 `.yaml`, 79 `.R`, 10 `.csv`, 2 `.yml` |
| Sentinel IDs that resolve to nothing on purpose | 4, in 32 occurrences in `tools/` tests and `tools/rules/check.py` |
| Registry causes citing a REQ ID; a card; nothing | 320; 52; 2, of 374 |
| Distinct (phase, condition, citation) triples | 363 today, 343 after the rewrite, citing 215 cards |
| Negative `error.yaml` with `requirement:` | 189: 147 REQ IDs, 42 card IDs |
| Run-report goldens with REQ IDs in RULE | 4 (CHECK-5, CHECK-7 and CHECK-13 after the rewrite) |

The four report goldens: `schema-adsl-age-quality/expected/adsl-violations.csv`
(REQ-0377), `schema-row-count-fractions/expected/adsl-checks.csv` (REQ-0385,
6 times), `schema-verification-log/expected/adsl-checks.csv` (REQ-0375,
REQ-0377, REQ-0385) and `schema-parquet/expected/adsl-violations.parquet`
(REQ-0377, in a binary file).

The plan, one pull request after the owner answers Q1: fix the registry
citations below; add a `citations` subcommand that rewrites every REQ ID in
authored text files, refuses an unresolvable ID other than the sentinels, and
lists binary files; rename the field in the facade (`Diagnostic.requirement`),
core (`Definition.requirement`), adapters, host tests, `tools/conformance/`,
`tools/benchmark.py`, the allowed keys of
`tools/validation/repository_manifests.py` and the 189 `error.yaml` files;
drop the `Replaces` lines, the legacy ID pattern of `tools/rules/model.py`
and `check_diagnostics.py`, and `rules/README.md`'s paragraph on `Replaces`;
rename the generated "requirement index" page; edit the three CSV report
goldens (10 values) and re-author the Parquet one independently.

Two expected changes are not mechanical: `negative-tied-baseline` and
`negative-baseline-unresolved-tie` would read `rule: EXPR-1` by the mapping,
and should read WINDOW-9, which states the tie failure.

### Registry citations

After the rewrite, 36 causes on 21 cards cite a card whose Fails line lacks
the condition (finding F-2). They fall into three classes:

- **Add the condition to the cited card**, which states the failure (22
  causes): FORMULA-20 (2), LOOKUP-17, LOOKUP-18 (`invalid_aggregate_context`,
  `mixed_relations`), LOOKUP-19 (`invalid_aggregate_context`, 2), LOOKUP-22,
  LOOKUP-7 (`unknown_field`), WINDOW-7, EXPR-11 (`incompatible_input_type`),
  EXPR-13 (`incompatible_input_type`, `invalid_field_type`), FORMULA-23
  (`incomparable_sources`, `incompatible_input_type`), TIME-4, TIME-7 and
  TIME-8 (`incompatible_input_type`), DEFINE-6 and DEFINE-7
  (`value_not_permitted`), DEFINE-12, and TEXT-1 (`unknown_field` from
  `str_extract`, or re-cite it to NAME-3).
- **Re-cite schema-constraint causes** to SCHEMA-12 or SCHEMA-13, which already
  name those conditions (13 causes): VALUE-1's `SchemaColumnType*`
  (`invalid_size`, `minimum_length`, `pattern_mismatch`), TIME-8's
  `SchemaTimeRule*` (4), TIME-10's `SchemaDayRule*` (3) and LAYER-4's
  `SchemaLayer*` (`invalid_field_type`, `missing_required_field`,
  `unknown_field`).
- **Re-cite one catch-all** (1 cause): `DatasetBaselineAmbiguity` cites
  REQ-0322, which EXPR-1 absorbed; WINDOW-9 states the tie and names
  `ambiguous_baseline`.

Eight negative benchmarks touch these causes. Six keep the mechanical
mapping, because the condition joins the mapped card's Fails line; the two
baseline benchmarks move to WINDOW-9.

## Rules tightening (#3048, P-RULE-10)

### NAME-5 and EXPR-5

NAME-5's title says "every other plain string is literal"; EXPR-5's title
says a bare string derivation "is only a source reference". A plain string in
`derivation` is literal by one and a reference by the other; the prose
reconciles them, the titles do not. D18 and D31 change both cards.

**NAME-5** (title 10 words, statement 69 words):

> A `variable` field names exactly one variable, never an expression
>
> An operand field typed `variable` accepts a concise source variable or a
> constructed output variable; an operation cannot place a nested expression
> in it, and operations compose through a named derived column. A
> `string_template` placeholder binds its complete name as a `variable` field
> does, and text outside placeholders is literal. Whether a plain string in
> any other field is a name or text follows from its field's kind (EXPR-5).

**EXPR-5** (title 16 words, statement 79 words):

> An expression field reads a bare name as a reference and quoted text as a
> literal
>
> A `derivation`, a `derivations` entry, `then`, `otherwise`, a handled
> wrapper's `value`, a key match value and a function argument are expression
> fields: each holds one expression of the closed grammar (FORMULA-1), in
> which a bare name is a reference, quoted text such as `'DM'` is a literal,
> and a YAML number, Boolean or null is a literal. A forgotten quote therefore
> fails validation as an unknown name. A value field holds a constant, and
> its bare text is text.

The [field-kind table](field-kinds.md) belongs with D31's card. If the owner
wants the contradiction gone before D18 lands, interim titles: NAME-5 "A
`variable` field names one variable, and a plain string in a value field is
literal"; EXPR-5 "A bare string in a derivation or case result is the source
shorthand, so it names a variable".

### Clearer titles

| Card | Today (words) | Proposed (words) | Why |
| --- | --- | --- | --- |
| FORMULA-14 | NULL propagates through every operator and function except NULLIF (9) | A missing operand makes arithmetic and numeric functions missing, except NULLIF's second argument (13) | AND, OR and IS NULL follow FORMULA-4, as the detail admits, and `NULLIF(3, missing)` is 3; the rules say "missing" |
| FORMULA-24 | A reduction skips missing values, and a group with nothing to reduce yields missing (14) | Value reducers skip missing values, and a group with no record reduces to missing (14) | `ONLY` counts records (FORMULA-20), and `COUNT(D.*)` reads no values |
| TIME-5 | diff's bounds counts the endpoints of a day range and applies only to dates counted in days (17) | `closed` sets which endpoints a day count between two dates includes, and only that count takes it (17) | Says what the field chooses; spelled with P-ORD-2's `closed` |
| WINDOW-9 | baseline_flag marks the last eligible row ordered by date and then by its tie_break terms (15) | baseline_flag flags the last eligible row of each partition in date order, and a tie for last fails (18) | States the failure, and survives P-ORD-1 |
| CHECK-5 | `not_missing` passes only when every value is present and is not declared on a key; every other value check passes missing values (22) | Only `not_missing` rejects a missing value, and declaring it on a key column is redundant (15) | One sentence for its two rules |
| SUB-10 | `core` uses its family's designations, and `mandatory` resolves from the keys, its declaration and the dataset's class (17) | `core` takes its model's designations, and `mandatory` comes from the keys, the declaration, or the class and `core` (18) | Gives the order in which `mandatory` resolves; "model" per P-MODEL-1 |
| INPUT-8 | A missing value is recognized before typing, and no text spells it (12) | A null or an empty field is missing, and no other text spells missing (14) | "Recognized before typing" is implementation language |

Statements that change with them:

> **FORMULA-14** (73 words). An arithmetic operator or a numeric function
> with a missing operand yields missing, with one exception: `NULLIF(x, y)`
> yields `x` when only `y` is missing (FORMULA-13). A derivation therefore
> needs no guarding predicate to survive a missing input, and a formula that
> must yield missing rather than fail uses `NULLIF`: percentage change against
> a zero base is `100 * (VALUE - BASE) / NULLIF(BASE, 0)`. Predicates and their
> logical operators follow FORMULA-4 instead.

> **FORMULA-24** (74 words). Inside a reduction's argument a missing operand
> propagates, so a record whose operand is missing contributes a missing value
> rather than a zero: `SUM(EX.EXDOSE * EX.EXDUR)` skips a record missing
> either factor. `SUM`, `COUNT(x)`, `MIN`, `MAX` and `MEAN` then ignore
> missing values, while `ONLY` counts records (FORMULA-20). A group with no
> record after `filter` yields missing. When every value is missing, `SUM`,
> `MIN`, `MAX` and `MEAN` yield missing, never zero, and `COUNT(x)` yields `0`.

> **INPUT-8** (65 words). In a delimited source, a field with no characters,
> bare or quoted, is missing whatever its declared type, never a parse
> failure, and no type admits an empty string from a delimited source
> (INPUT-9). No text is a missing-value sentinel: `NA`, `NULL`, `.`,
> `unknown` and every other spelling are ordinary string values. What missing
> is, and how each operation treats it, is the missing-values card.

### A headline card for missing values

`rules/values.md` has a "Missing values" topic holding only VALUE-4, while
about 30 cards state what an operation does with missing; P3 names four.
**New VALUE card** (title 17 words, statement 83 words), landing with D18:

> Missing is one value distinct from every other, and each operation states
> what it does with it
>
> Missing is the absence of a value. It is distinct from every string, number
> and the empty string, every column type admits it, and no text spells it. A
> specification writes it as YAML null in a value position, and a source
> supplies it as a null or an empty field. Every operation states what it does
> with a missing input, and the table below names the rule that fixes each
> treatment: comparison, arithmetic, reduction, handlers, verification, keys,
> functions and the artifact profiles.

| Operation | Treatment | Rule |
| --- | --- | --- |
| Comparison | `UNKNOWN`; `IS NULL` tests for missing | FORMULA-4 |
| Arithmetic and numeric functions | missing, except NULLIF's second argument | FORMULA-14 |
| Reductions | skipped; an empty group is missing; `COUNT(x)` of only missing values is 0 | FORMULA-24, FORMULA-20 |
| `case`, `first_available`, `greatest`, `least` | no branch gives missing; the first present source; the extreme of the present sources | EXPR-7, EXPR-9, EXPR-10 |
| Local handlers | an omitted handler gives missing for a missing single input or an absent record | COLUMN-10, COLUMN-11 |
| Lookups | absence gives missing unless it must match | LOOKUP-24 |
| Windows | an excluded row gets missing; `locf` carries present values | WINDOW-4, WINDOW-8 |
| Dates and times | partial text and a missing source give missing, unless a fallback is declared | TIME-2, TIME-13, TIME-19 |
| Conversion | a missing input is not converted | VALUE-3 |
| Non-finite floats | become missing | VALUE-4 |
| Order | missing sorts lowest unless `nulls` places it | VALUE-22 |
| Verification | every check but `not_missing` passes missing; `all_or_none`; `unique` counts missing | CHECK-5, CHECK-10, CHECK-11 |
| Keys and mandatory columns | never missing | OUTPUT-3, SUB-11 |
| Project functions | a missing argument short-circuits unless its parameter accepts missing | FUNC-7, FUNC-9 |
| ODM reads | no identified record gives missing | NAME-13 |
| Inputs | a null or an empty field; no sentinel text | INPUT-8, INPUT-9, CSV-14 |
| Artifacts | an empty CSV field, a Dataset-JSON `null`, SAS Transport blanks or a period | CSV-4, JSON-11, XPT-7, XPT-8 |

P3's "Enforced by" list adds the new card.

### Principle P11: a package is consistent and traceable

`rules/README.md` lists P1 to P10, and a proposal that conflicts with one
changes the principle first. P11 names what the submission rules already
enforce across artifacts, and gives the staleness guard of #3048 its
principle:

> **P11. A package is consistent and traceable.** The datasets, transport
> files, run reports and Define-XML documents a project publishes come from
> one set of resolved specifications and agree on every fact they share. Every
> published fact names its source: an identifier derives from declared
> identifiers, metadata from the specification that built the data, and an
> origin is declared, never inferred. Enforced by JSON-5, JSON-6, DEFINE-9,
> SUB-19 and INPUT-11.

### FUNC-6's null argument against LAYER-9 (resolved by D18)

FUNC-6 lets a call argument be "a missing YAML scalar", while LAYER-9 makes
YAML null remove a field, so `args: {x: null}` would remove the argument
instead of passing missing. D18 resolves it. **FUNC-6** (title 14 words,
statement 54 words):

> A call argument is one leaf of the expression grammar, never a composed
> expression
>
> A call argument is one expression leaf: a bare name for a variable, quoted
> text, a YAML number or Boolean, YAML null for missing, or a `DATE` or
> `DATETIME` literal whose text is a Values value. No argument may contain an
> operator, a function call or another expression, and a name resolves under
> FORMULA-9.

**LAYER-9** (title 19 words, statement 79 words), following P-CLR-1:

> YAML null is the missing value, and a layer clears an inherited field only
> by listing it in `clear`
>
> YAML null is the missing value wherever a value or an expression may be
> missing, and it never removes anything. A layer clears an inherited value
> only by listing its spec path in the root field `clear`; composition removes
> the value before the layer's own fields compose, and a field with a schema
> default takes it again. A path that names nothing the layer inherits, a
> required or identity field, `yamaa`, `type` or a keyed member fails as
> `invalid_clear`.

COLUMN-10's paragraph on `{literal: null}` and null handlers is rewritten in
the same change.
