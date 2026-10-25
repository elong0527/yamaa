# Method and converter footprint

Part of the [1.0 interface RFC](../interface-1.0.md). Every number in the
RFC was measured at main `1758ad145` (the merge of #3099). The three drafts
behind it were first measured at `ec90332cf`; rerun at `1758ad145`, no
benchmark count moved, and a few test counts did (for example 281 intermediate
`no_match` in all instead of 280, and 181 `compute` instead of 175).

The measurement scripts are not committed. They read the repository and
write nothing into it, and `tools/` holds checks and tools that `make` and CI
run, not one-off measurements. This page describes each method precisely
enough to repeat.

## The corpus walker (spelling, structure and footprint)

1. **Inventory.** Load the three schema bundles under `yaml/` as SCHEMA-1 to
   SCHEMA-10 describe them (classes, `fields_from`, registries, unions) and
   list every field, registry keyword and enumerated value with its shortest
   path from the file root: 356 (owner, field) pairs, 210 enumerated values and
   46 registry keywords.
2. **Extraction.** Pull YAML documents out of the corpus: YAML files; fenced
   `yaml` blocks in Markdown; Rust string literals (normal, raw and `format!`
   strings, rebuilt with a placeholder for each interpolation) and `json!`
   bodies; JSON and TSV fixtures that embed specs; Python string literals,
   f-strings and dict displays, through `ast`; R string literals, `c(...)`
   line vectors and `paste0(...)` runs. Load each under the YAML 1.2 core
   schema (SCHEMA-4).
3. **Classification.** Classify each document (spec, project, environment,
   codelists, function, dictionary, a fragment of one, harness metadata,
   other), and anchor a fragment, such as a column list in a rules card, at the
   schema position it fits best.
4. **Walk.** Walk each document against the schema, attributing every key to
   its owner (a named class or a registry payload such as `expressions.diff`),
   recording which union member each value matched, every enumerated value,
   and every unknown key. No positive benchmark spec has an unknown key.
5. **Counts.** Group occurrences by area: `benchmarks/` (files and README
   blocks), Rust (tests, sources, examples and `rust/tests/host-fixtures`),
   hosts (`python/`, `R/`), docs (`docs/`, `README.md`, `CONTRIBUTING.md`,
   `rules/`, `yaml/README.md`) and tools (`tools/`, `evaluations/`,
   `automation/`).
6. **Inferred keys (D23).** For each benchmark entry spec, composed with its
   parents: classify each qualified read as a driver read, a join, an
   intermediate read or an aggregate (shorthand `aggregate: "SUM(EX.X)"` and
   mapping form alike); a qualified read of an input that is not the reading
   template's driver is a join, including a join on rows another template's
   driver built (NAME-4). The applicable keys are the output `keys` that the
   relation carries, with the relation's fields read from its CSV header (all
   140 cases), Parquet schema or `schema` producer.
7. **Rename table.** Merge the inventory and counts with the hand-written
   decisions and proposals into `tools/migrate/renames.yaml`, and generate the
   field-kind page from it.

Limits: Rust tests that build documents from tuples are not parsed, so their
tokens are counted instead; 94 Rust and 47 host strings use interpolation the
extractor could not rebuild.

## The submission walker (CDISC alignment)

Load benchmark YAML files, fenced YAML in Markdown and inline YAML in Rust,
Python and tools sources (1,198 literals looked like YAML documents; 1,067
parsed), and count `submission` fields, origins, value-level entries, project
sections, standards, documents and codelists. Copy candidates read the
producer named by `schema` and compare type and label. Where-clause codelist
checks resolve the where column's binding in the composed spec.

The CDISC draft's own join count (69 pairs in 62 specs, 34 keyless
aggregates) came from a simpler token scan, which counted a shorthand
aggregate's operands and prose such as a method description as joins, and
missed joins inside row templates whose driver differs. The RFC uses the
walker's counts above: 59 join pairs, 55 keyless qualified aggregates, 86
input declarations.

## The rules and API scripts

- **Conditions.** Compare `yaml/conditions.yaml` with every card's Fails line
  (read through `tools/rules/model.py`), the core registry's causes (parsed
  from the `conditions!` block of `rust/crates/yamaa-core/src/diagnostic/mod.rs`),
  the negative benchmarks' `error.yaml` and the crate sources; rewrite each
  REQ citation to its card through `Replaces` and list causes whose card's
  Fails line would lack their condition.
- **Phases.** Collect every phase and stage vocabulary from the registry, the
  rules prose, COLUMN-10's handler table, `engine_rejected` contexts and the
  facade's progress enums.
- **Citations.** Count REQ IDs in authored files (as `tools/rules/model.py`
  selects them) by area and file type, and the field name `requirement`.
- **API surface.** Read Python's public names and members with `ast`, R's
  exports and methods from `NAMESPACE` and its sources, and the native surface
  from `rust/interface/bindings.yaml`.
- **Card text.** Check every proposed title and statement against the card
  limits with `tools/rules/model.py`'s word count.
- **Bindings draft.** Load `rust/interface/bindings-1.0.yaml` and check that
  every native export is paired or host-only, every operation names existing
  exports, the issues and report columns equal section 2.7 and CHECK-24, and
  every value vocabulary a field names exists.

## Converter footprint

Occurrences and documents in each area (the last row gives documents and
files):

| Change | Benchmark files | Benchmark READMEs | Rust | Host suites and fixtures | Docs blocks | tools |
| --- | --- | --- | --- | --- | --- | --- |
| header (D4) | 484 / 484 | 1 / 1 | 1,146 / 103 | 463 / 32 | 12 / 7 | 20 / 9 |
| `literal` (D18) | 891 / 167 | 9 / 5 | 517 / 364 | 94 / 74 | 11 / 5 | 3 / 3 |
| `{literal: null}` in value fields (D18) | 156 / 95 | 13 / 11 | 152 / 131 | 57 / 48 | 9 / 9 | 0 |
| intermediate `no_match` (P-HND-1, P-HND-3) | 97 / 68 | 7 / 7 | 120 / 111 | 52 / 49 | 5 / 5 | 0 |
| origins (P-ORIGIN-1) | 145 / 28 | 4 / 4 | 34 / 16 | 9 / 3 | 4 / 2 | 0 |
| `compute` (D18) | 84 / 55 | 5 / 5 | 70 / 61 | 14 / 13 | 8 / 8 | 0 |
| intermediates without `key` (D23) | 31 / 27 | 16 / 16 | 61 / 57 | 43 / 42 | 11 / 11 | 0 |
| project `environments` (D8) | 8 / 8 | 1 / 1 | 54 / 54 | 20 / 20 | 2 / 2 | 1 / 1 |
| environment `functions` (D8) | 16 / 16 | 0 | 35 / 35 | 9 / 9 | 2 / 2 | 0 |
| `not_missing: {}` (P-HND-4) | 58 / 38 | 2 / 2 | 9 / 8 | 0 | 1 / 1 | 0 |
| `rows[].derivations: {}` (P-HND-4) | 9 / 9 | 1 / 1 | 24 / 22 | 8 / 7 | 0 | 0 |
| `baseline_flag` operands (P-OP-2, P-OP-3) | 18 / 9 | 6 / 3 | 8 / 4 | 34 / 17 | 2 / 1 | 0 |
| section `standard` (D16) | 17 / 17 | 0 | 9 / 8 | 1 / 1 | 4 / 2 | 0 |
| `minimum_source_precision` (P-IMP-1) | 15 / 11 | 5 / 5 | 5 / 4 | 1 / 1 | 3 / 3 | 0 |
| `{source: X}` in wrappers (D18) | 10 / 9 | 2 / 2 | 9 / 9 | 2 / 2 | 2 / 2 | 0 |
| aggregate `derive` (P-BIND-1) | 10 / 7 | 0 | 10 / 10 | 5 / 5 | 0 | 0 |
| `study_day.date` (P-OP-1) | 14 / 11 | 3 / 3 | 7 / 7 | 1 / 1 | 0 | 0 |
| `datasets` (P-PRJ-1) | 14 / 14 | 1 / 1 | 4 / 4 | 1 / 1 | 0 | 0 |
| `diff.bounds` and `cut.right` (P-ORD-2) | 17 / 14 | 0 | 5 / 5 | 0 | 0 | 0 |
| `tie_break` (P-ORD-1) | 2 / 2 | 2 / 2 | 1 / 1 | 0 | 1 / 1 | 0 |
| any change: documents / files | 484 / 484 | 59 / 52 | 1,250 / 104 | 493 / 32 | 39 / 12 | 22 / 9 |

The negative `error.yaml` files hold no documents; `evaluations/harbor`
holds no specification (its prompts are prose and its solutions programs);
`automation/` holds none.

Spec-path strings with a changed segment, occurrences / files:

| Segment | Negative `error.yaml` | Rust | Host suites and fixtures | Docs | tools |
| --- | --- | --- | --- | --- | --- |
| rows and intermediates by index (P-CLR-2) | 37 / 35 | 577 / 76 | 120 / 4 | 2 / 2 | 0 |
| lookup `.no_match` (P-HND-1) | 1 / 1 | 190 / 24 | 83 / 3 | 1 / 1 | 1 / 1 |
| `.compute` (D18) | 8 / 8 | 85 / 27 | 7 / 2 | 0 | 1 / 1 |
| `.literal` (D18) | 1 / 1 | 26 / 6 | 3 / 3 | 0 | 0 |
| `study_day.date` (P-OP-1) | 1 / 1 | 3 / 3 | 2 / 2 | 0 | 0 |
| `diff.bounds` (P-ORD-2) | 1 / 1 | 1 / 1 | 2 / 2 | 0 | 0 |
| `.reference_date` (P-OP-3) | 0 | 0 | 7 / 1 | 0 | 0 |
| `aggregate.derive` (P-BIND-1) | 0 | 6 / 6 | 1 / 1 | 0 | 0 |
| section `standard` (D16) | 0 | 4 / 4 | 0 | 2 / 2 | 0 |
| `submission.origin` (P-ORIGIN-1) | 0 | 3 / 2 | 0 | 0 | 0 |
| section `datasets` (P-PRJ-1) | 0 | 1 / 1 | 0 | 0 | 0 |

The 189 negative `error.yaml` files hold 196 spec paths: 37 change under
P-CLR-2 and 12 under D18 and the renames. All 189 also carry `requirement`
(D28, Q1).

Token counts in every text file: `schema_version` 485 in 484 benchmark files,
1,375 in 157 Rust files, 471 in 37 host files, 36 in 17 docs files and 21 in
11 tools files (and 2 in Harbor task files, where it is Harbor's own key);
`no_match` 662 in 66 Rust files and 256 in 7 host files.

## What a mechanical rename cannot convert

1. **Inferred keys (D23):** 26 intermediate keys and 86 input declarations in
   benchmarks need input headers; in tests, 61 Rust, 43 host and 11 docs
   intermediates omit `key` over inputs that test code writes, so the
   converter reads headers out of that code or lists the specs for repair.
2. **Child-layer nulls** become `clear:` entries whose paths need the layer's
   keyed context (2 in benchmark files, 10 in tests).
3. **D8 merges** need both host files; a test that writes only a Python
   environment produces a function with one implementation.
4. **`model`** has no source for 10 CDISC and TEST benchmarks, nor for any
   test spec; P-MODEL-1 makes it optional there.
5. **Negative benchmarks whose fault disappears or changes:**
   `negative-impute-missing-month` and `negative-impute-unreachable-month`
   (P-IMP-1), `negative-version-mismatch` (D4), `negative-literal-structure`
   (D18), `negative-lookup-unknown-key` (D23), and
   `negative-property-clear`, whose path becomes `clear[0]`.
6. **Interpolated and builder-built Rust documents.**
7. **Fixtures that pair a spec with its expected diagnostics** (the JSON and
   TSV fixtures under `rust/crates/yamaa-application/tests/fixtures`, and
   `rust/tests/host-fixtures/cases.json`) change both halves together.
8. **Code that reads or writes the surface:** `tools/benchmark_docs/mapping_doc.py`
   renders `literal`, `compute` and `no_match`;
   `tools/staging/stage_specification_inputs.py` writes
   `minimum_source_precision`; `tools/conformance/public_conformance.py`
   writes `schema_version` and reads `literal`; `tools/benchmark.py` and
   Harbor's build and scaffold scripts read `benchmark.yaml`'s `standard`;
   `python/src/yamaa/style` checks `literal_form` and `redundant_intermediate`.
9. **Rule and doc prose** that cites a renamed field is edited by hand.
