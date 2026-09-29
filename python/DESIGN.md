# DESIGN.md  --  clean-room engine architecture

Current stage: **Stage 3  --  IO** (all stages complete; the engine now tracks
schema-vocabulary growth inside the same architecture).
Stage-1 exit criterion: 100% of benchmark specs reproduce their goldens.
Stage 1 complete 2026-09-21: 205/205 in-scope benchmarks pass (4 ODM
failures out of scope; 7 skips = 4 Stage-2 function benchmarks + 3
non-CSV inputs). Stage 2 complete 2026-09-21: all 4 function benchmarks
pass (`function:` derivations with `environment.yaml` contracts/bindings,
REQ-0698/0699/0700 validation, REQ-0008 non-finite normalization).
Stage 3 complete 2026-09-21: all 3 IO benchmarks pass (parquet input via
REQ-1034/1035/1036, REQ-0852 unknown-profile, REQ-0785
not-regular-file). ODM remains out of scope.

## Goal

One spec  ->  one output dataset. The engine is a row-oriented interpreter:
a declarative YAML derivation is validated, planned into a column DAG,
then executed. Values are Python scalars (None = missing); temporal values
use `YDate`/`YDateTime` wrappers.

**Note (2026-09-20):** The original design targeted Polars with no
row-at-a-time loops. The as-built implementation uses Python dicts/lists
with row-wise evaluation for clarity and debuggability within the 6-hour
box. This is a documented deviation; a Polars rewrite remains future work.

## Module layout (as built)

- `api.py`  --  entry points: `derive(spec_path) -> str` (CSV text) and
  `derive_artifacts(spec_path) -> {path: text | bytes}`, every file a run
  publishes (storage/publication).
- `errors.py`  --  `YamaaError` with phase/condition/requirement/spec_paths.
- `values.py`  --  scalar model: `YDate`, `YDateTime`, missing/present,
  comparison and equality.
- `csv_io.py`  --  CSV profile scanner and serializer (storage/csv),
  Parquet reader and writer (storage/parquet).
- `pred.py`  --  predicate tokenizer/parser/evaluator (operations/predicates).
- `numeric.py`  --  numeric expression parser/evaluator
  (operations/computation).
- `agg.py`  --  aggregate expression parser and reduction evaluator
  (operations/aggregation).
- `expr.py`  --  expression registry: one function per registered
  expression (operations/expressions).
- `engine.py`  --  spec loading, input binding, row construction
  (execution/rows), column derivation in dependency order, lookups
  (operations/lookup), windows, conversion (values/types), verification
  (execution/verification), output rendering (storage/publication).
- `validate.py`  --  Stage-1 shape validation: inputs, columns, output,
  lookups, rows, expressions, dependencies, regex, verifications, paths.
- `logs.py`  --  the warning and verification logs (execution/verification):
  sidecar declarations, each declared check's outcome and complete evidence,
  and the two verified sidecar datasets.
- `compose.py`  --  specification composition (specification/composition):
  resolves an entry's `parents` chain into the resolved specification the
  engine then loads, reading declared kinds from the `yaml/` bundle.
- `workflow.py`  --  producing-specification workflows (storage/ingestion):
  an input with `schema` runs its producer first, in the same run, and
  reads the artifact it renders.

## Data flow

```
spec.yaml  ==>  engine.py (load, bind inputs)
  expand named windows (REQ-1251/1252/1253): root `windows:` shapes
  validated, string `window:` references replaced by independent
  deep copies of their definitions  --  before semantic analysis
  validate.py (shape checks; raises YamaaError)
  engine.py: row construction  ->  column DAG  ->  evaluate  ->  verify  ->  CSV
```

Row construction without `rows` (REQ-0042) is two-phase:
- Phase A: per input record, derive the key columns whose derivations
  contain no window function (`_KeyCtx`: base-qualified sources only).
- Phase B: derive window-based key columns over the phase-A key table
  (`_KeyWinCtx`): unqualified variables read phase-A key values,
  base-qualified variables read the input record; anything else is a
  forward reference (REQ-0074). Then dedupe on the full key combination
  in first-appearance order.
Window evaluation itself is phase-agnostic: `_BaseCtx.window_value`
partitions/orders whatever rows the context exposes via `_win_n()` and
`_win_val()`; the column phase and the key phase only differ in what
those hooks read. Every window kind is one branch of `_window_kind`
(kind, ordered positions, engine): `locf` (REQ-1239) reads a separate
*completed* output column, so the source is a plain unqualified variable
and naming the column being derived is a dependency cycle; it walks each
ordered partition returning the current source value when present, else
the closest strictly earlier non-missing source  --  always an original
observed value, never an already-filled output. `window.filter`
excludes both donor and recipient rows before partitioning (excluded
recipients read missing). The five order-requiring kinds
(`row_number`, `rank`, `row_value`, `previous_non_missing`, `locf`)
fail validation as `window_order_by_required` (REQ-0340) when
`order_by` is absent.

## Invariants

1. Every derivation is total: any input yields a value or None  -- 
  the engine raises `YamaaError` (never a Python exception) on spec or
  data problems.
2. Missing propagates explicitly: None is the only missing representation.
3. Column references resolve at plan time via dependency DAG; cycles and
  forward references fail validation.
4. One expression node = one YAML dict key; adding a key adds one eval
  function plus one validation branch.
5. Window functions never see phase: `_win_n()`/`_win_val()` are the only
  row access `window_value` uses, so key-phase, column-phase, and
  row-template-phase windows share one implementation.
6. Row templates with window derivations are staged (REQ-0326): phase A
  derives the derivations that neither are nor read a window, per
  constructed row; then each stage evaluates the windows whose inputs are
  complete over the template's constructed rows (`_RowWinCtx`:
  unqualified variables read completed row values, qualified variables
  read the row's own input record), and derives, row by row, the
  derivations those windows complete. A window may therefore read another
  window's result, directly or through scalar derivations.
7. A failure names where the specification wrote what failed. While an
  expression evaluates, `ctx.where` is its own path, so an operation's
  failure is at `<derivation>.<operation>` (or a field under it); a row
  template is `rows[i]`, and a column-level default a template inherits is
  still `columns.<name>.derivation`. A failed conversion is the column's
  (`columns.<name>`). A failure evaluating one row carries that row's
  output `keys` once every key is derived; a window failure carries its
  partition's.
8. Schema friction is recorded in FINDINGS.md, never entrenched.

## What the design refuses

- No ODM layer (being deleted, issue #506).
- No reading of the existing implementation; no `yamaa` import.
- Approved data roots (REQ-0769..0772): the clean-room runs in the
  REQ-0771 "packaging or conformance" mode  --  no runner-supplied roots,
  so every `project_path` (input paths, a `mapping.dict` path) resolves inside
  the spec's directory as the single approved root. A runner that
  passes roots would select among them before spec reading; the
  resolution call is the one place that change lands.

## Vocabulary notes (schema catch-up, same architecture)

- `mapping.dict` (REQ-1110): the dictionary is written inline or as the
  path of a YAML project resource holding it. A path is read once per
  run, cached on the engine; its content must satisfy the inline `dict`
  contract.
- Driver-correlated lookup filters (REQ-0120/0132/0133): an
  intermediate `filter` may qualify fields with the
  current driver dataset (root `base`, or the sole input). Donor-only
  filters still evaluate once per run; a correlated filter evaluates
  per current row against the equality-matched donor records, before
  range narrowing and ordered selection, keeping only TRUE rows.
  `order_by` stays donor-only. Filter identifiers must be qualified;
  anything outside donor+driver scopes fails `unknown_field`.
- Bare-string case results (`case_result: [str, expression]`): a `then:`
  / `otherwise:` written as a bare string normalizes to `{source: ...}`,
  exactly like a bare column derivation (REQ-0266/REQ-0319).
- `COUNT(D.*)` (REQ-0484/0505): the star names its relation explicitly,
  so it dispatches through the qualified aggregate path with the named
  dataset as the relation; the star name is never a value identifier.
- `str_case` (REQ-1114, `to: sentence`/`title` under REQ-1240/1241):
  ASCII-only casing via the REQ-0708 substitutions  --  never a host
  `capitalize` routine; a non-string source fails
  `incompatible_input_type` like every other text operation.
- Match values are scalars (REQ-0115): the current-row side of a
  lookup's or an aggregate's `key` pairs (the same-named value for a
  list, the value beside each column for a mapping) and a `between`
  value resolve through the context's scalar value semantics. A
  qualified variable naming the row's own input group collapses to its
  single carried value (REQ-0075), and a joined read with more than one
  record and no `order_by`/`keep` fails `join/multiple_matches`
  (REQ-0127). A key column is a bare column of the relation (REQ-0116,
  REQ-0141).
- Implicit join (REQ-0111/0133/0136): a structured `source` keeps its
  `filter` and `order_by`/`keep` on the join. The join reads through an
  equality index built once per dataset and key list (`_join_records`),
  whose keys equal exactly when REQ-0005 compares the values equal, so a
  read costs one probe rather than a scan (issue #1485).
- Ungrouped row filters (REQ-0036/0068): the filter reads the driver
  record, the candidate's derived columns, and lookup state. It gates the
  record once the derivations it reads are complete; a discarded record
  derives nothing else.
- Field types (REQ-0517/1032): the Parquet schema types its fields through
  the exact closed mapping (anything else fails
  `source_field_type_unsupported`); an undeclared CSV field is `str`, never
  inferred from its values.
- Grouped row-template aggregates declare no key pairs (REQ-0142): a
  grouped row aggregate reads its own input group  --  the group is the
  match  --  so `key` there fails validation as
  `invalid_aggregate_context`.
- Intermediate derivations (REQ-1185, amended by #1072): an intermediate's
  `derivations:` map evaluates in declaration order: each derivation is
  computed once per donor record, and a derivation may read the dataset's
  stored fields plus the derivations declared before it (bare or
  dataset-qualified). A derivation may use a window function; the window
  partitions the donor records as augmented by every earlier derivation,
  so its `group_by`, `order_by`, and `filter` may read a stored field or
  an earlier derived name. A non-window derivation may read another
  input-backed intermediate's column (REQ-1263): the read runs that
  intermediate's match from the donor record, once per record. A
  reference to a driver field, a derivation declared later in the same
  map, or an unstored name fails `unknown_field`; reading a `SELF`
  intermediate fails `phase_boundary`, and intermediates that read each
  other fail `dependency_cycle`. Shadowing a stored column
  fails `duplicate_derivation`. Derived values augment the donor record
  before `filter`, matching, `order_by`, `columns`, and
  `verification.unique`, behaving like stored fields downstream.
- Intermediate uniqueness (REQ-1245): `verification.unique` asserts a
  column combination unique across the source-filtered, derivation-
  augmented donor records before any row is built; a duplicate fails the
  run as `duplicate_intermediate_records`. A driver-correlated filter
  admits no run-wide donor set and cannot combine with `verification:`.
- `str_contains` (REQ-1243/1244): the sole Boolean function the
  predicate grammar admits  --  `str_contains(source, pattern)` with a
  string-literal portable-regex pattern, UNKNOWN on missing source,
  `incompatible_input_type` on non-str source, `invalid_predicate` on a
  rejected pattern or any other `name(...)`. Like every predicate that
  does not parse, those report REQ-0188 with the predicate and the
  position where parsing stopped. The same key exists as a
  column expression returning bool with a `missing` handler.
- `to_date` (REQ-1107/REQ-0607): source is a `datetime`, ISO 8601 date
  text, or ISO 8601 datetime text (its calendar date); a `date` is not
  accepted as an identity spelling. Other text is `impute`/
  `invalid_date_text`, the temporal stage REQ-0348 names.
- Named windows (REQ-1251/1252/1253): root `windows:` declares named window
  specs by identifier (`dict[identifier, window_spec]`); an expression's
  `window` accepts a string naming one. At load, definitions are
  shape-validated (a non-mapping definition, a reference chain, or a `ref`
  form fails schema validation) and every string reference is replaced by
  an independent deep copy of its definition  --  before dependency analysis,
  so the expansion keeps the reference's caller scope (a row template's
  own constructed rows included). An undeclared name fails `unknown_window`
  at the expression's `window` field with the name in context `window`.
  Names compare exactly.
- `date_impute` month policy (REQ-0592): `month` is required when
  `minimum_source_precision` is `year` (the default) and must be absent
  when it is `month`; violations fail validation as `month_required`
  (context `minimum_source_precision`) / `month_not_permitted` (context
  `month`), checked before range checks.
- Rename-only intermediates (REQ-1248): an intermediate declaring only
  `id`, `dataset`, and at most `no_match: null` merely renames the
  dataset qualifier and fails
  validation as `rename_only_intermediate` at `intermediates[i]`.
- `cut` missing-input phase (REQ-0334): a `cut` whose numeric source is
  missing with no `missing` handler fails as `cut`/`missing_input`, not
  `mapping`/`missing_input`.
- `flag` (REQ-1256/1257/1258): shorthand for the common one-branch `case`.
  Payload is a bare predicate string (condition; `true_value` defaults to
  `"Y"`, `false_value`/`missing` absent) or a mapping with
  `condition` (required, a valid predicate), `true_value` (default `"Y"`),
  `false_value`, `missing`. TRUE selects `true_value`, FALSE
  `false_value` (missing when absent), UNKNOWN `missing` (missing
  when absent)  --  an unknown condition never falls through to
  `false_value`, unlike `case` with `otherwise`. A `false_value` without
  `missing` fails validation as `missing_value_required` at
  `missing` (REQ-1258). Neither a predicate string nor a mapping,
  a missing/non-predicate `condition`, or an invalid predicate fails
  validation (`invalid_field_type` / `invalid_predicate`).
- `str_pad` (REQ-1261): `source` (any present scalar) plus a positive
  integer `width`; converts the source to its canonical text (REQ-0010:
  identity / decimal text / shortest positional float text / ISO temporal
  text; booleans fail conversion) then pads left with ASCII spaces to at
  least `width` characters, never truncating. Missing source yields
  missing; a non-integer width or a width below one fails
  `invalid_field_type`.
- `key` expressions (REQ-1259): the match value beside a column in a
  `key` mapping may be an expression instead of a variable; it evaluates
  against the
  current row and its value is the match operand for that position (a
  missing result matches nothing, REQ-0131). The pair's REQ-0118
  comparison uses the expression's statically known result type, or no
  static type when the operation states none. REQ-0117: every identifier
  an expression entry reads must name a known current-row value
  (`unknown_field` otherwise).
- Row catalogs (REQ-1249) are retired without replacement: row_class is
  closed (REQ-0285), so a template declaring `catalog` fails validation as
  `unknown_field` at `rows[i].catalog`.
- `mapping` unmapped result (REQ-1110): a present source with no
  dictionary entry returns the `unmapped` result when declared and
  otherwise fails `mapping/unmapped_value`; `missing` never answers it.
- `unresolvable_name` diagnostic (REQ-0189): an unqualified identifier
  naming a field of an in-scope dataset fails `unresolvable_name` with
  the qualified spelling as `suggestion`; any other unavailable
  identifier fails `unknown_field`. A `case` `when` carries its exact
  nested predicate site, and an unknown predicate identifier is re-keyed
  by `identifier` at that site.
- Row-construction named-intermediate reads (REQ-0126): a row template
  derivation may read `<intermediate>.<column>` for a named intermediate.
  The intermediate matches per driver row with row-construction match
  semantics (bare names read the row under construction, group keys, or  -- 
  for ungrouped templates  --  the driver record's own fields; qualified
  names of the template's dataset read the driver record or group key).
  Matching reuses `_match_lookup_row` with a row-scoped cache identity.
- `key` expression operands in row construction (REQ-0126/1259):
  expression entries evaluate with match-operand semantics  --  bare operands
  resolve through `_row_match_value` (row, group keys, driver-record
  fields) rather than the REQ-0189 `value()` path. `_BaseCtx`
  exposes `_match_operand` (default `value()`); `_RowCtx` overrides it.
  Only `str_pad` reads operands through `_raw_operand`, so the override
  is scoped to it.
- Row-phase defaults (REQ-1260): a row-local column-level derivation  -- 
  no lookup, aggregate, or window; no named-intermediate read; every
  column it reads row-local  --  becomes that column's default derivation
  when at least one `rows` entry names the column, or when a row-phase
  context reads the column (a `rows` derivation, a grouped `rows` filter,
  a donor field of a `SELF` intermediate, or a match variable / `between`
  value of a named intermediate that a `rows` derivation reads). A
  default's own reads of column-level columns promote those derivations
  too (transitive closure). Validation computes the promoted set once
  (`e.row_defaults`); each row template derives the promoted columns it
  does not override locally, during row construction, and the column
  phase skips them as already derived.
- `SELF` intermediates (REQ-0120): a named intermediate may declare
  `dataset: SELF` when the spec has row templates. The donor pool is the
  completed derived rows: during row construction, the rows of earlier
  completed templates (a read from the first template fails
  `phase_boundary`); during column derivation, all completed rows. Donor
  fields are the output columns derived in every row template plus the
  REQ-1260-promoted columns (`e.donor_fields`). In a `SELF`
  intermediate's `filter`/`order_by`, bare names and `SELF.<field>` name
  donor fields (bare names are never correlated current-row reads); the
  driver qualifier names the driver dataset's fields (root base or sole
  input). `SELF` may be read only through a named intermediate, never as
  a directly qualified source. Omitted `key` matches on the applicable
  output keys carried by the donor fields. The donor pool is never
  cached across template boundaries (REQ-0136); `verification.unique` on
  a `SELF` intermediate runs over the donor pool after each completed
  row template and fails the run as `duplicate_intermediate_records`.
