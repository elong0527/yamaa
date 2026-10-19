# Spelling review (section 2.6)

Part of the [1.0 interface RFC](../interface-1.0.md). #3030 section 2.6
asks the RFC to measure each spelling candidate and propose a rename table
(D27); the owner prefers continuity, so every rename below states its reason
(I4: ambiguity, inconsistency, CDISC misalignment or duplication) and every
option table includes keeping today's spelling. Counts: "bench" is
`benchmarks/` files and README blocks, "rust" the Rust tests, sources and
host fixtures, "hosts" the Python and R suites, "docs" `docs/`, `rules/` and
`yaml/`. Measured at main `1758ad145`.

## Operands (P-OP-1 to P-OP-3)

**Question.** One name for an operation's one input and one for several
(`source`, `sources`), replacing `variable`, `date`, `item`, `start` and
`end` where only one is needed, and `value` in `baseline_flag`?

| Operand | bench | rust | hosts | docs | bench files |
| --- | --- | --- | --- | --- | --- |
| `source` on 17 verbs (`str_*`, the temporal verbs, `row_value`, `locf`, `mapping`, `cut`) | 268 | 258 | 5 | 15 | 230 |
| `first_available` / `greatest` / `least` `.sources` | 27 / 8 / 11 | 33 / 8 / 3 | 1 / 0 / 0 | 1 / 0 / 0 | 23 / 7 / 10 |
| structured binding `source: {variable}` | 46 | 113 | 1 | 4 | 40 |
| order term `{variable}` | 33 | 24 | 0 | 4 | 31 |
| `study_day.date`, `study_day.reference` | 17 | 8 | 0 | 0 | 14 |
| `baseline_flag.date`, `.reference_date`, `.value` | 12 each | 21 each | 0 | 1 | 9 |
| `diff.start`, `diff.end` | 24 | 9 | 0 | 4 | 22 |
| `odm: {item}` / bare `odm: D.I` | 2 / 37 | 5 / 34 | 0 | 1 / 2 | 1 / 36 |
| `between.value` | 32 | 33 | 0 | 3 | 24 |
| `impute.not_before` / `not_after` / `fallback` | 2 / 2 / 1 | 2 / 4 / 0 | 0 | 0 | 2 / 2 / 1 |
| `shift.days` / `amount` | 3 / 2 | 11 / 12 | 0 | 0 | 3 / 2 |

The convention already holds: 17 single-input verbs call their input
`source`, and the three several-input verbs call them `sources`.

| Id | Today | 1.0 | Reason |
| --- | --- | --- | --- |
| P-OP-1 | `study_day.date` | `source` | the verb's one input; `date` is also a column type, a temporal-literal keyword and an `imputation_flag.part` value |
| P-OP-2 | `baseline_flag.value` | `source` | the analysis value the baseline is for is the verb's input (WINDOW-9); `value` also names the handled wrapper, `between.value` and the codelist item value |
| P-OP-3 | `baseline_flag.reference_date` | `reference` | the role and spelling `study_day.reference` already has |

Kept, with reasons:

- `variable` in the structured binding and in order terms: the binding is
  itself `source`, so renaming its field gives `source: {source: X}`; it is
  also the schema type the field holds (NAME-5).
- `odm.item`: an ItemOID reference written `DATASET.ItemOID`, not a variable
  (NAME-7), in ODM's own word.
- `diff.start` and `end`: two roles whose order gives the sign (TIME-4); no
  verb takes only one of them.
- `between.value`: the current-row value whose forms #2934 just decided (P1,
  P3).
- `baseline_flag.date`, the dtc parts, the impute bounds and `fallback`,
  `shift.days` and `amount`: distinct roles.

Cost: 41 occurrences in benchmarks, 93 in all; 1 negative path.

## Handlers and absence (P-HND-1 to P-HND-5)

| Handler | Omitted gives | bench | total | `{literal: null}` | restates the default | other values |
| --- | --- | --- | --- | --- | --- | --- |
| `mapping.missing` | missing | 35 | 58 | 31 | 31 | text 19, number 7, null 1 |
| `mapping.unmapped` | fails | 22 | 38 | 16 | 0 | text 17, number 5 |
| `cut.missing` | missing | 5 | 13 | 6 | 6 | text 7 |
| `str_extract.missing` | missing | 9 | 15 | 11 | 11 | text 4 |
| `str_extract.no_match` | fails | 7 | 15 | 7 | 0 | text 8 |
| `str_replace.missing` | missing | 3 | 3 | 3 | 3 | |
| `str_case.missing` | missing | 6 | 24 | 12 | 12 | text 12 |
| `str_template.missing` | fails | 4 | 11 | 5 | 0 | text 6 |
| `flag.missing` | missing, required beside `false_value` | 19 | 44 | 11 | 1 | text 32, number 1 |
| `flag.false_value` | missing | 20 | 53 | 2 | 0 | text 48 |
| `first_available.missing` | missing | 9 | 28 | 0 | 0 | text 14, number 14 |
| `impute.invalid` / `precision.invalid` | fails | 15 / 3 | 22 / 4 | 22 / 4 | 0 | |
| derivation `unconvertible` | fails | 12 | 35 | 16 | 0 | number 14, Boolean 4, text 1 |
| `source.absent` | missing after a join; fails for an unknown variable | 1 | 12 | 4 | 4 | text 5, number 3 |
| `intermediates[].no_match` | missing | 104 | 281 | 225 | 225, plus 37 YAML nulls | number 9, text 6 |
| `require_match` on a binding / intermediate / input | | 1 / 57 / 1 | 2 / 79 / 3 | | | |

**P-HND-1. One name per meaning.** Two names carry two meanings each.
`no_match` is a lookup with no candidate on an intermediate (LOOKUP-24) and a
pattern that did not match on `str_extract` (TEXT-10). `absent` is a join
with no record on a source binding (LOOKUP-24), and also, under COLUMN-10's
bind row, a variable that does not exist. The rules call lookup absence
"absence" (LOOKUP-24's title, `conflicting_absence_policy`).

| Option | Change | Cost |
| --- | --- | --- |
| a | keep | none |
| b | `no_match` for every lookup, a new `unmatched` for `str_extract` | 27 |
| **c** | **`absent` for every lookup; `str_extract.no_match` kept** | **15 values renamed after P-HND-3** |

Of 281 intermediate `no_match`, most restate the default in single-layer
specs and are deleted (P-HND-3); 11 sit in child layers (7 overrides become
`absent: null`, 4 clears become `clear:` entries) and 15 hold another value.
190 Rust, 83 host and 1 negative path strings end in a lookup's `.no_match`.
The family then reads: `absent` (nothing to read), `missing` (an input is
missing), `no_match` (a pattern did not match), `unmapped` (no dictionary
entry), `invalid` (unreadable text), `unconvertible` (the conversion failed).
The run report's handler rows for lookups follow the same name (LOOKUP-14);
no golden pins them.

**P-HND-2. One absence policy.**

| Option | Form | Assessment |
| --- | --- | --- |
| **A** | **One answer field (`absent`: a literal, or null for missing) and `require_match: true` for "fail"; declaring both fails as `conflicting_absence_policy`** | **Kept as today** |
| B | One policy field: `absent: fail`, `absent: null` or `absent: {value: L}` | Needs a reserved word in a value field, which collides with the text "fail", or the `{value: L}` wrapper D18 just removed |

B would also retire `conflicting_absence_policy` and its negative benchmark.

**P-HND-3. Restated defaults.** Handler restatements of a missing default:
about 320 in all. Schema-default restatements: 106 in all, led by codelist
`data_type: text` (22), section `context: Submission` (16 of 19 uses),
`cut.right: false` (10), `minimum_source_precision: year` (6), input
`empty_string: missing` (6) and `xml_lang: en` (5 of 5 uses).

| Option | Form |
| --- | --- |
| a | admit silently, as today |
| **b** | **a lint finding (`restated_default`) with `--fix`** |
| c | a validation refusal, like `redundant_long_form` |

A restated default is harmless and sometimes documents intent, and a refusal
would break files for no semantic gain. The converter deletes
`{literal: null}` restatements as part of D18, in single-layer specs only: in
a child layer the line overrides an inherited answer, and deleting it would
inherit the parent's.

**P-HND-4. Empty mappings.** 70 documents write `- not_missing: {}` (60 in
benchmarks), and 42 write `derivations: {}` on a row template that derives
nothing row-locally (10 in benchmarks), because `row_class.derivations` is
required.

| Option | `not_missing` |
| --- | --- |
| a | keep `not_missing: {}` |
| **b** | **the bare keyword `- not_missing`, admitted when the payload declares no required field; `{}` becomes its refused long form** |
| c | `not_missing: true` |

#2836 plans optional fields for `not_missing`, so the payload must stay a
mapping when fields are written, which (c) cannot do. `rows[].derivations`
becomes optional.

**P-HND-5. An unknown variable answering `absent`.** COLUMN-10's handler
table has a bind row: with `absent` declared, a variable that does not exist
in context answers the literal instead of failing. P1 says nothing is
inferred, and I9 says fail instead of choosing. No benchmark relies on it:
the only benchmark that declares `absent`, `schema-handlers`, reads a variable
that exists. **Recommendation (Q9):** remove the bind row, so an unknown name
always fails (NAME-3) and `absent` answers only a lookup with no record.

## Per-record bindings (P-BIND-1, P-BIND-2)

| Shape | bench | rust | hosts | docs |
| --- | --- | --- | --- | --- |
| row-template `derivations`: map of column to derivation (1,877 entries, 638 templates) | 980 | 874 | 15 | 8 |
| intermediate `derivations`: map of field to derivation or `{type, derivation, unconvertible}` | 35 | 146 | 6 | 14 |
| aggregate `derive`: list of `{name, type, derivation}` (37 bindings in 24 aggregates) | 13 | 22 | 2 | 0 |

| Option | Change |
| --- | --- |
| a | keep three shapes |
| **b** | **one map shape keyed by name: aggregate `derive` becomes `derivations: {NAME: {type: T, derivation: E}}`, `type` still required there (LOOKUP-22); row-template entries stay untyped, since the column declares the type** |

A map composes by key under LAYER-6, while today's list is replaced whole;
evaluation is in declaration order in both. **P-BIND-2:** the untyped
intermediate wrapper `{value: E}` cannot carry `unconvertible` (LOOKUP-15), so
it only restates `E`, as the handled wrapper's equivalent is refused
(SCHEMA-15); 2 uses, both in tests.

## Temporal fields (P-IMP-1)

55 `impute` uses. The common combinations are `type: date` with `month` and
`day` and the default minimum (12, plus 4 with `invalid`), and `type:
datetime` with `minimum_source_precision: day` and `time` (9 with `invalid`,
8 without). `minimum_source_precision` is written 29 times (day 17, month 6,
year 6, the year ones restating the default). TIME-10 requires `month`
exactly when the minimum is year, and `day` exactly when it is year or month,
so the minimum always follows from the rules supplied: month and day give
year, day alone gives month, neither gives day.

| Option | Form |
| --- | --- |
| a | keep the fields flat |
| b | group them, keeping the minimum: `completion: {minimum: month, day: first, time: first}` |
| **c** | **drop `minimum_source_precision`; keep `month`, `day` and `time` flat** |
| d | group and drop the minimum |

Declare each fact once (I2): impute completes only the parts the spec says
how to complete, so a source missing an unsupplied part gives missing, as
today. Cost: 29 deletions. The change retires `month_required`,
`month_not_permitted`, `day_required` and `day_not_permitted`, and the two
negative benchmarks that test only that redundancy,
`negative-impute-missing-month` and `negative-impute-unreachable-month` (Q3).
A spec that writes neither `month` nor `day` becomes valid with minimum day,
an admission and not a break. If no benchmark should retire, (a) keeps
today's surface; grouping (b, d) adds a level for little gain.

## ODM reads (P-ODM-1)

| Idiom | bench | rust | docs | bench files |
| --- | --- | --- | --- | --- |
| grouped-row `aggregate` with `filter: ODM.ItemOID = '...'` and `ONLY(ODM.Value)` | 101 | 1 | 0 | 18 |
| `odm: D.I` | 37 | 34 | 2 | 15 |
| structured source binding with `filter: ODM.ItemOID = '...'` | 29 | 0 | 0 | 8 |
| named intermediate over the ODM input, keyed on hierarchy fields | 21 | 0 | 0 | 5 |
| row-template `filter` on `ItemOID` | 5 | 5 | 0 | 5 |
| `odm: {item, event, form, item_group, filter}` | 2 | 5 | 1 | 1 |

The idioms read the same thing: the `Value` of one item record at a scope.
Three take the scope from the row (NAME-10); the intermediate idiom declares
it with its key. Their failures differ when several records match:
`odm_not_unique`, `ONLY`'s failure, or `multiple_matches`.

| Option | Change |
| --- | --- |
| **a** | **Keep the four idioms in the benchmarks; teach `odm` as the one idiom in the docs; reserve `odm.scope`** |
| b | Add `odm.scope` (subject, event, form, item group) and convert the benchmarks by hand where equivalent: 38 files |

The spelling draft recommended (b). This RFC recommends (a), for the owner
to choose (Q12): the four idioms are general constructs that stay valid in
any case; the conversion is not mechanical, since equivalence holds only on
data where the multiple-match failure does not arise and an intermediate keyed
on output columns reads rows not built from ODM; and the docs can teach one
idiom without rewriting benchmarks. `scope` is reserved, so it can arrive in
1.x as an optional field (I5).

## Overloaded words (P-WORD-1)

| Word | Field | bench | rust | hosts | docs | Recommendation |
| --- | --- | --- | --- | --- | --- | --- |
| standard | section `standard` | 17 | 9 | 1 | 4 | `standards` (D16) |
| standard | codelist source `standard` | 6 | 2 | 0 | 3 | keep: Define-XML writes CT as a def:Standard of type CT, and the field holds one |
| environment | project `environments` | 9 | 54 | 20 | 2 | keep (D8's spelling) |
| domain | root `domain` | 426 | 1,213 | 118 | 10 | keep |
| datasets | section `datasets` | 15 | 5 | 0 | 0 | `transport` (P-PRJ-1) |
| dataset | `intermediates[].dataset`, `rows[].dataset` | 290 | 628 | 14 | 18 | keep |
| type | column, origin, temporal verbs, function parameter, page, typed bindings, root file type (D4) | | | | | keep |
| intermediates | root `intermediates` | 128 | 315 | 11 | 15 | keep |

- `domain` holds the output dataset's name, which CDISC calls the dataset
  name; `submission.domain` holds the domain. The difference matters for split
  datasets and ADaM, but 1,785 uses, PROJECT-5's spec names and every harness
  read argue for keeping it.
- `type` is tied to its class in each position, the field-name and
  descriptor-keyword distinction of SCHEMA-5, as Define-XML uses Type for
  origins, methods and page references.
- `intermediates` against `lookups`: the rules call the read a lookup
  (LOOKUP-25), but an intermediate can also drive row templates (ROW-2; 7
  template references in 4 benchmarks), which is not a lookup, and conditions
  and the report use the word. Keep it, and make the schema description say
  "named intermediate".

## Ordering and bounds (P-ORD-1 to P-ORD-3)

| Field | bench | rust | hosts | docs |
| --- | --- | --- | --- | --- |
| `output.order_by` | 45 | 4 | 2 | 1 |
| window `order_by` | 95 | 100 | 2 | 6 |
| intermediate `order_by` (with `keep`) | 52 | 87 | 7 | 9 |
| source binding `order_by` (with `keep`) | 16 | 26 | 0 | 1 |
| `baseline_flag.tie_break` | 4 | 1 | 0 | 1 |
| `diff.bounds` | between 1, exclusive 1, inclusive 8 | | | |
| `cut.right` | false 7 | false 3, true 2 | | |
| `baseline_flag.inclusive` | false 1 | false 1 | | |

**P-ORD-1.** `baseline_flag` orders its eligible rows by `date` and then by
`tie_break` (WINDOW-9), and WINDOW-5 forbids `window.order_by` for it.

| Option | Change |
| --- | --- |
| a | keep |
| b | rename `tie_break` to `order_by` on the verb, which leaves two `order_by` levels in one verb |
| **c** | **admit `window.order_by` for `baseline_flag`, defaulting to `[date]`, and fold the tie-break terms into it: `tie_break: [ATM, LBSEQ]` becomes `window.order_by: [ADT, ATM, LBSEQ]`** |

Eligible rows have a non-missing date, so prepending `date` ascending
reproduces today's order. Cost: 6 occurrences. `window_order_by_forbidden`,
which only this verb raises and no negative pins, retires. Both benchmark
uses declare an inline window; a `baseline_flag` that named a window shared
with other verbs would need its own copy, which the converter would list for
manual repair.

**P-ORD-2. One way to state endpoint inclusion.**

| Option | Change |
| --- | --- |
| a | keep `bounds: exclusive / inclusive / between`, `right: true / false` and `inclusive: true / false` |
| b | `closed: left / right / both / neither` on all three |
| **c** | **`closed` on the two interval fields; `inclusive` kept on the bound** |

`diff`'s day count: `exclusive` counts (start, end], so `closed: right`, the
default; `inclusive` becomes `both`, `between` becomes `neither` (TIME-5).
`cut`'s intervals: `right: true` becomes `closed: right`, and the default
`right: false` is `closed: left` (EXPR-13). `baseline_flag` compares one date
with one bound, where `inclusive: false` reads more plainly than
`closed: neither`. Cost: 22. `closed` is the interval vocabulary of pandas and
Polars, available to later range features.

**P-ORD-3. Equality matches as key matches.** No lookup filter compares a
donor field with a current-row value by equality (0 matches in intermediate,
aggregate and binding filters). Equality with a constant is pervasive: 180 of
212 benchmark aggregate filters, 64 of 81 intermediate filters and 36 of 42
binding filters have such a conjunct. State the rule that an equality between
a donor field and a current-row value is a key pair, as the lint finding
`equality_filter` rather than a refusal, since nothing violates it; leave
constant equalities in `filter`, since moving them into `key` would restyle
280 filters and make keys hold constants.

## Long forms (P-LONG-1)

No positive benchmark, doc or host test writes a long form; they appear only
where the refusal is tested and in fixtures of the canonical resolved form
(LAYER-14).

| Class | Shorthand written | Class form written |
| --- | --- | --- |
| order term | 613 | 61 |
| source binding | 269 | 168 |
| input declaration | 915 | 1,396 |
| aggregate | 129 | 303 |
| flag | 82 | 57 |
| `str_template` | 70 | 11 |
| `odm` | 73 | 8 |
| handled wrapper | 8,219 | 73 |

The class form is written only where it adds a field.

| Option | Form |
| --- | --- |
| **a** | **refuse in validation, and `lint --fix` rewrites** |
| b | admit long forms and let lint canonicalize |
| c | refuse, as today, with no fix |

Under 2.11 a refused form can be admitted in 1.x, but an admitted form can
never be refused again, so the strict choice keeps both doors open, and
`lint --fix` gives people and agents the repair. D18 removes SCHEMA-15's
`{source: X}` row, and P-HND-4 adds `not_missing: {}`.

## Project file (P-PRJ-1 to P-PRJ-3)

**P-PRJ-1.** `datasets: json / xpt` is used 20 times (json 9, xpt 11; 15 in
benchmarks).

| Option | Spelling |
| --- | --- |
| a | keep |
| **b** | **`transport: xpt / dataset_json`, #2855 decision 18's spelling** |
| c | `transport: xpt / json` |

The owner chose the spelling in decision 18, the field selects a transport
format rather than datasets, and `json` alone does not name Dataset-JSON,
whose 1.1 also defines an NDJSON representation. JSON-1 and XPT-1 change.

**P-PRJ-2.**

| Field | Uses |
| --- | --- |
| `define` | 33 |
| `standard` | 31 |
| `datasets` | 20 |
| `context` | 19 (16 restate the default) |
| `source_system_version` | 18 |
| `documents` | 14 |
| `xml_lang` | 5 (all restate the default) |
| `file_oid`, `metadata_version` | 4 each |
| `originator` | 3 |
| `stylesheet` | 0 |

Options: flat (recommended); grouped under `define: {path, ...}`, which turns
the scalar into a mapping; grouped under a new field. D3 keeps the shape,
PROJECT-3 already says which fields only Define-XML reads, and `documents` and
`standards` are also read when a spec is validated, so no group could hold
everything Define-only.

**P-PRJ-3.** `alias` holds the NCI concept code of a codelist (8 uses) or an
item (18), written to Define-XML as an Alias with Context nci:ExtCodeID; CDISC
Controlled Terminology publishes the same value as "Code". The spelling draft
proposed `code` only if the CDISC alignment reserved `alias` for Define-XML
aliases in general. It does not (the domain description of split datasets is
derived), so `alias` stays, with Define-XML's element name.

## Unused surface (P-UNU-1)

| Surface | Measurement | Recommendation |
| --- | --- | --- |
| `encoding` on non-SAS inputs | 47 uses, none in benchmarks: 43 on SAS inputs in Rust tests, 1 on a CSV input, 3 in docs and hosts | keep it SAS-only: SAS-2 requires it on every SAS or Transport input, and CSV stays UTF-8 (D15) |
| `stylesheet` | 0 parsed uses; 15 Rust builder tokens; DEFINE-23 | keep: the only way to link a define.xml stylesheet |
| codelist file paths | 45, none in benchmarks | keep, as `type: codelists` (P-TYPE-1) |
| file-backed function definitions | 28, none in benchmarks | removed by D8: each becomes a function file |
| mapping dictionary files | 8 | keep, header-less (P-DICT-1) |
| `relative_to` | 18 on inputs and 17 on outputs, all in tests | stays (D5) |
| fields and values with no use | 25 fields with no parsed use, 27 more used only outside the benchmarks, 78 enumerated values | keep |

A removed field cannot come back in 1.x without a new optional field, while an
unused optional field costs nothing; most unused items are Define-XML 2.1
attributes, ISO 8601 parts or CDISC vocabulary values. The fields with no
parsed use: `submission.has_no_data` and `subclass`; the `severity` of
`all_or_none`, `allowed_values` and `max_length`;
`imputation_flag.seconds_collected`; `dtc.hour`, `minute`, `second` and
`invalid`; the value-level `core`, `role`, `display_format`,
`inventory_vocabulary`, `method` and `comment`; the structured method's
`name`, `type`, `description`, `expression` and `documents`, and its formal
expression's `context` and `code`; `pages.title`; and the section's
`stylesheet`.
