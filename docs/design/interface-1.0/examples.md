# Converted examples

Part of the [1.0 interface RFC](../interface-1.0.md) (#3030 Phase 0
item 4). Six benchmarks converted by hand to the proposed 1.0 surface, applying
the [rename table](../../../tools/migrate/renames.yaml) exactly and writing
out today's inferred join keys. They show what Phase 2's converter would
produce and where it needs more than the table.

- **Not runnable.** No engine reads the 1.0 surface yet. The files sit under
  `docs/`, carry no inputs, and keep each benchmark's relative paths
  (`input/...`, `define.xml`), so they read as if they replaced the files in
  the benchmark directory. No benchmark harness, test or conformance plan
  reads them.
- **Only converted files are shown.** Data files (`input/`, `expected/`, the
  R lock) and harness metadata (`benchmark.yaml`, `README.md`) are unchanged
  and not copied; Phase 2 decides whether `benchmark.yaml`'s `standard` gives
  way to the spec's `model`.
- **Semantics are unchanged.** Each conversion keeps the benchmark's meaning,
  so Phase 1's front-end must reproduce each benchmark's expected bytes from
  these files.
- `schema-inheritance` has no child-layer null, so its conversion writes no
  `clear:`; its sibling `schema-inheritance-clear` is converted too, to show
  `clear:`.

Compare each file with its benchmark, for example
`diff benchmarks/adam-adtte-pfs/spec.yaml docs/design/interface-1.0/examples/adam-adtte-pfs/spec.yaml`.

## sdtm-dm-metadata

[`project.yaml`](examples/sdtm-dm-metadata/project.yaml),
[`spec.yaml`](examples/sdtm-dm-metadata/spec.yaml)

| Change | Entry |
| --- | --- |
| `schema_version: "1.0"` becomes `yamaa: "1.0"` and `type: spec` or `type: project` | D4 |
| `model: SDTM`, required: the spec declares `submission` and the `sdtm` section lists it | P-MODEL-1 |
| `derivation: {literal: DM}` and `{literal: YEARS}` become `"'DM'"` and `"'YEARS'"` | D18 |
| Nine `origin:` mappings become one-entry `origins:` lists | P-ORIGIN-1 |
| `standard:` becomes a one-entry `standards:` list, whose first entry is the default | D16 |
| `datasets: json` becomes `transport: dataset_json` | P-PRJ-1 |

Unchanged: the inline codelists and their `alias` fields (P-PRJ-3);
`context: Submission` and `xml_lang: en`, which restate defaults and become
lint findings, not converter deletions (P-HND-3); the string template, the
verifications and the `submission` block. No join: `DM_RAW` is the only input
and the driver. Manual work: none. The define.xml keeps `STD.SDTMIG` and
`STD.CT_SDTM`, and each origin still renders as one def:Origin.

## adam-adtte-pfs

[`spec.yaml`](examples/adam-adtte-pfs/spec.yaml)

| Change | Entry |
| --- | --- |
| Header, and `model: ADaM` from `benchmark.yaml`'s `standard` (no section lists the spec, but the converter has a source) | D4, P-MODEL-1 |
| `key: [STUDYID, USUBJID]` on the intermediates `PROGRESSION`, `DEATH` and `LASTASSESS`: today's applicable keys, the output keys (`STUDYID`, `USUBJID`, `PARAMCD`) that `rs.csv` and `ds.csv` carry | D23 |
| `key: [STUDYID, USUBJID]` on input `RS`, because two qualified aggregates over it (`PRENTXDT`, `PRENTXSEQ`) omit `key`; `DS` gets none, since only the `DEATH` intermediate reads it | P-JOIN-1, P-KEY-1 |
| Three `no_match: {literal: null}` deleted: each restates the default in a single-layer spec | D18, P-HND-3 |
| 16 literals become quoted text (`"'CENSORED'"`), numbers (`then: 0`) or `null` | D18 |
| `bounds: inclusive` becomes `closed: both` | P-ORD-2 |

Unchanged: the aggregates' `group_by` and `between`, the `case` structure,
and the comments. Manual work: the keys come from the input headers, which
the converter reads. Under option (a) of P-KEY-1, each of the two aggregates
would write `key: [STUDYID, USUBJID]` instead of the input.

## sdtm-lb-multiform

[`spec.yaml`](examples/sdtm-lb-multiform/spec.yaml)

| Change | Entry |
| --- | --- |
| Header, and `model: SDTM` from `benchmark.yaml` | D4, P-MODEL-1 |
| 61 literals: 42 become quoted text, 3 numbers and 16 `null` | D18 |
| `then: {compute: {expr: "2 + FORMREPEAT / 100"}}` becomes `then: "2 + FORMREPEAT / 100"` | D18 |
| `value: {source: LBORRES}` becomes `value: LBORRES` in the handled wrapper | D18 |
| `unconvertible: {literal: null}` becomes `unconvertible: null`, kept: the default is to fail, so it restates nothing | D18 |

Unchanged: the six row templates' ODM idiom (a grouped-row `aggregate` with
an `ItemOID` filter and `ONLY`), which P-ODM-1 keeps; the `mapping` dictionary
values and `allowed_values`, which are value fields; the row templates'
`derivations` maps. `LBLOC: null` in a row template is the missing value, as
`{literal: null}` was; it is not a handler, so it is not deleted. No join:
`ODM` is the only input. Manual work: none.

## adam-adsl-bmi

[`project.yaml`](examples/adam-adsl-bmi/project.yaml),
[`functions/bmi.yaml`](examples/adam-adsl-bmi/functions/bmi.yaml),
[`spec.yaml`](examples/adam-adsl-bmi/spec.yaml). Removed:
`python/environment.yaml` and `r/environment.yaml`. Kept in place:
`r/renv.lock`.

| Change | Entry |
| --- | --- |
| The two environment files merge into `functions/bmi.yaml`: a header, `name: bmi` from the map key, and `implementations` from each host's `function`; every other field is identical in both files and is taken once | D8 |
| The R lock moves to the project as `environments: {r: {lock: r/renv.lock}}`, rewritten relative to the project file; the Python file declared no lock, so there is no `python` entry | D8 |
| The project lists `functions: [functions/bmi.yaml]` | D8 |
| Spec header, and `model: ADaM`, required because the `adam` section lists it | D4, P-MODEL-1 |
| `derivation: {compute: {expr: ...}}` becomes the expression text | D18 |

Unchanged: the call's arguments (`weight_kg: WEIGHTKG` is a bare name, a
reference before and after; `cm_per_m: 100` a YAML number), and the test
cases, whose `null` arguments and results already mean missing; no test value
is tagged, so P-FN-1 does not apply. Manual work: the location
`functions/<name>.yaml` is the converter's rule, not a table entry. The spec's
comment that "the runner picks the project root that implements it" describes
today's harness, which runs each host with its own environment file; the
converter keeps comments, and Phase 2 rewrites this one with the harness.

## schema-inheritance

[`spec_organization.yaml`](examples/schema-inheritance/spec_organization.yaml),
[`spec_compound.yaml`](examples/schema-inheritance/spec_compound.yaml),
[`spec_study.yaml`](examples/schema-inheritance/spec_study.yaml)

| Change | Entry |
| --- | --- |
| A header on every layer; each layer declares `type: spec` | D4 |
| `model: ADaM` on the entry layer only, the specification `benchmark.yaml` names; parents inherit it through composition | P-MODEL-1 |
| `derivation: {literal: unused}` becomes `"'unused'"` | D18 |

Unchanged: `unused_reference` already declares `key: [USUBJID]` and
`require_match: true`; the compound layer's input `types`; the `metadata`
maps. No join (`LB` is the base), and no child-layer null, so no `clear:`.
Manual work: none.

## schema-inheritance-clear

[`spec_organization.yaml`](examples/schema-inheritance-clear/spec_organization.yaml),
[`spec_study.yaml`](examples/schema-inheritance-clear/spec_study.yaml)

| Change | Entry |
| --- | --- |
| Headers, and `model: ADaM` on the entry layer | D4, P-MODEL-1 |
| The study layer's `PD: null` becomes `clear: [columns.DCSREAS.derivation.mapping.dict.PD]`, written after `parent` | D18, P-CLR-1 |
| The `DCSREAS` column entry, empty once the null leaves, is dropped, and with it the layer's `columns` list | D18 |

The clear removes the inherited `PD` entry before the layer composes; the
other three entries and the inherited `unmapped: OTHER` remain, so a PD code
is still OTHER. Manual work: the comment that explained the null is rewritten
for `clear:`; the converter keeps comments but cannot rewrite their text.

## What the converter needs beyond the table

The six cases confirm the rename table's transforms and add these
requirements for Phase 2:

- Read input headers to write today's applicable keys (`adam-adtte-pfs`),
  and skip intermediates that drive row templates (D23-R1).
- Put each function file at `functions/<name>.yaml` beside the project file,
  and rewrite lock paths relative to the project file.
- Write `model` on the entry layer only, from the listing section or
  `benchmark.yaml`.
- Drop a column entry, and then a `columns` list, that a clear leaves empty.
- Keep comments, and list those that mention a converted construct for
  review.
- Delete `{literal: null}` handlers only where they restate a missing default
  in a single-layer spec.
