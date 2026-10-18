# YAML derivation specification

This folder defines the language-agnostic specification for ODM-to-SDTM and
SDTM-to-ADaM derivations. The design is under active development.

## Start here

The [contract index](../rules/README.md#rule-files) organizes the language by semantic owner:

| Block | Subject |
| --- | --- |
| Specification | Structure, composition, and binding |
| Values | Types, numbers, text, and temporal values |
| Execution | Lifecycle, rows, ordering, handlers, and verification |
| Operations | Expressions, languages, lookup, windows, and functions |
| Storage | Resources, ingestion, CSV, Parquet, and publication |
| Submission | Metadata, terminology, and Define-XML |
| Reference | Schema notation for maintainers |

## Sources of authority

Schemas own structure, defaults, and structural constraints. Indexed contracts
own shared and operation-local behavior. Schema descriptions link to the owning
requirement. Examples demonstrate those contracts without redefining them.

Closed grammars are defined once in [grammar/](grammar/README.md). Their
contract blocks are generated views checked against those files; R and Python
replay the same parser vectors.

## Contents

| Location | Purpose |
| --- | --- |
| [schema.yaml](schema.yaml) | Specification entry point |
| [schema_environment.yaml](schema_environment.yaml) | Study environment entry point |
| [rules/](../rules/README.md) | Normative contracts |
| [Schema fields](https://elong0527.github.io/yamaa/reference/schema-fields/) | Generated shapes, defaults, and semantic links |
| [Requirement index](https://elong0527.github.io/yamaa/reference/requirements/) | Generated canonical IDs and owners |
| [benchmarks/](../benchmarks/README.md) | Specifications, inputs, and expected outcomes |
| [conformance/](conformance/) | Shared language fixtures |
| [grammar/](grammar/README.md) | Closed grammars and parser vectors |

## Execution overview

The [lifecycle](../rules/columns.md) owns execution order. Read it
first when implementing a runner, then follow the contract for each stage.
[Define-XML generation](../rules/profiles/define-xml.md) is a separate workflow
that composes specifications and metadata without running their derivations.

## Review workflow

1. Read the [schema notation](../rules/reference/schema-language.md), the relevant
   schema entry point, and its transitive includes.
2. Read the owning contracts and their dependencies.
3. Inspect representative positive and negative specifications, input data,
   and expected outcomes.
4. Update fixtures when behavior changes and require equivalent R/Python
   outcomes within their implemented coverage.
5. Run repository, migration, grammar, documentation, and conformance checks.

Unspecified behavior remains an explicit design question. Implementations
must not infer a new contract from a host default.

## Maintenance rules

Terminology and requirement allocation live in the
[contract index](../rules/README.md#terminology). When changing a schema,
rule, grammar, or fixture:

- Keep `schema.yaml` and every `schema_*.yaml` module compact and strictly valid
  YAML.
- Register every expression in an `expressions` registry and every verification
  in its applicable verification registry before an example uses it.
- Keep handler fields local to the expression or result stage that can use them;
  do not recreate a generic exception list.
- Store each cohesive semantic area in one rule file under `rules/`.
- Declare governed submission metadata in `submission`, never in the free-form
  `metadata` map. The [submission contract](../rules/profiles/submission.md)
  reserves its governed key names, so the map cannot
  become a second place a governed fact lives.
- Declare nothing a rule derives. For `sdtm` and `send`, `Mandatory` comes
  from `core`; for `adam` the two are independently declared. A submission
  length comes from `max_length`, an ADaM origin source from its type, and,
  under the `Other` context, a collected value's page-less annotated-CRF
  reference from the document that declares it.
- Declare a codelist once in the study environment and bind columns to its
  identifier, as required by the
  [terminology contract](../rules/profiles/terminology.md).
- Change a closed grammar in its `grammar/` file first. The rule's grammar
  block is rendered from that file, the shared parser's closed vocabulary is
  compared with it, and both hosts replay its vectors, so a change made
  anywhere else fails validation. Add a vector for every behavior the change
  decides, in each context that decides it.
- Keep repository-authored source ASCII-only. Spell non-ASCII characters by
  code point in rules, documentation, schemas, specifications, and tests;
  literal Unicode belongs only in input and expected-output data fixtures.
- Keep ordinary examples portable: declare source paths relative to the YAML
  file that writes them and store their fixtures beside that file. Explicit
  `relative_to: entry` examples keep study inputs beside each study entry. Dedicated
  path-policy tests may exercise parent-directory segments and temporary
  absolute local paths as positive cases. URI schemes, symbolic links,
  non-regular resources and missing files remain negative cases. A
  `yamaa-project.yaml` does not change document resource locations.
- Write every fixture under the source profile its extension selects. CSV
  fixtures follow the [CSV profile](../rules/reference/csv.md): UTF-8 without a
  byte-order mark, comma-separated
  fields, a named non-empty field per column, and the same field count in
  every record. Parquet fixtures follow the closed
  [Parquet profile](../rules/reference/parquet.md).
  A file that departs from its profile belongs only in a negative example that
  declares the condition it provokes.
- Do not duplicate normative behavior across schema definitions, rules, or
  examples. Keep all behavior in its owning contract and link to it from schema
  descriptions. Generate field tables with `generate_rule_reference.py`.

## Reusing a complete window

A specification can declare the same grouping, ordering, and selection once:

```yaml
windows:
  RESPONSE_ORDER:
    group_by: [STUDYID, USUBJID]
    order_by: [ADT, RSSEQ]
```

Use `window: RESPONSE_ORDER` in each expression that shares those settings.
An inline mapping remains valid. References use the whole definition and
accept no extra fields or overrides. Omitted settings keep their usual
meaning, and each use operates on its caller's rows. See the
[window contract](../rules/windows.md#window-1) and the
[confirmed-response benchmark](../benchmarks/adam-adrs-confirmed-response/spec.yaml).

## Carry-forward and donor-record selection

`locf` takes a completed `source` variable and a `window` with `order_by`.
It preserves a current value and fills gaps from earlier non-missing values
within the partition; it never constructs additional rows.

A named intermediate can compare a donor field with the
current driver's field in its `filter`, for example
`OBS.AVISITN < PLAN.AVISITN`. The driver is the row template's `dataset` (an
input dataset or an eligible named intermediate), or root `base` without row
templates; either defaults to the single declared input when omitted (see the
[contract index](../rules/README.md#terminology)). `order_by` and `keep` select one donor
from the surviving candidates. Use an explicit planning input for visits
that do not yet have observed records. See the [window contract](../rules/windows.md)
and [lookup contract](../rules/lookup.md).

## Explicit source record order

Declare `ordinal` on a CSV, Parquet, SAS, or Transport input to expose its original one-based
data record position as an `int` field:

```yaml
input:
  SOURCE:
    path: input/source.csv
    ordinal: SourceOrdinal
```

A sequence column can then state its ordering explicitly:

```yaml
derivation:
  row_number:
    window:
      group_by: [GROUP_ID]
      order_by: [SOURCE.SourceOrdinal]
```

Filters retain the original positions, while `row_number` assigns consecutive
numbers within each group. The generated name must be absent from the stored
fields. It is typed automatically and is emitted only through a selected
output column that derives it. Fixed-schema ODM inputs do not accept `ordinal`.
See the [source ingestion contract](../rules/inputs.md#input-3)
and [source ordinal benchmark](../benchmarks/schema-source-ordinal/spec.yaml).
