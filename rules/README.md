# Rules

> A yamaa specification has exactly one execution. Where it would have two,
> yamaa fails instead of choosing.

These rules fix that execution. Read the principles below, then the rules of
the area you are changing. Each rule is a card: an ID, a title that states the
rule, a short statement, the conditions it raises and the evidence that tests
it, followed by detail for implementers. The titles and statements are written
for review; the detail is written for implementation and must agree with
them.

## Principles

Every rule serves one of these principles. A proposal that conflicts with one
changes the principle first.

**P1. Everything is declared.** Columns, types, keys, joins, row construction,
order and checks are written in the specification. Nothing is inferred from
data, from file contents, from a same-named field or from a host default.
Enforced by [REQ-0092](specification/binding.md#req-0092),
[REQ-0198](specification/structure.md#req-0198),
[REQ-0525](storage/ingestion.md#req-0525),
[REQ-0716](storage/publication.md#req-0716) and
[REQ-0832](storage/ingestion.md#req-0832).

**P2. Values never convert silently.** An operation never converts its inputs.
A value converts once, to its column's declared type, by one table, and a
failed conversion is a named failure. Enforced by
[REQ-0004](values/types.md#req-0004), [REQ-0009](values/types.md#req-0009),
[REQ-0013](values/types.md#req-0013) and
[REQ-0527](storage/ingestion.md#req-0527).

**P3. Missing is a value.** Missing is distinct from every string, number and
the empty string; no text spells it, every type admits it, and every operation
states what it does with it. Enforced by
[REQ-0166](operations/predicates.md#req-0166),
[REQ-0344](execution/handlers.md#req-0344),
[REQ-0492](operations/aggregation.md#req-0492) and
[REQ-0530](storage/ingestion.md#req-0530).

**P4. Ambiguity fails.** Two values for one key, several matching records
without a declared selection, an unmapped value or text that does not parse
fails with a named condition instead of choosing. Enforced by
[REQ-0075](execution/lifecycle.md#req-0075),
[REQ-0127](operations/lookup.md#req-0127),
[REQ-0482](operations/aggregation.md#req-0482) and
[REQ-1272](specification/binding.md#req-1272).

**P5. Order is data.** Stored record order, construction order and declared
order terms decide every tie. No operation reorders records or chooses an
order of its own. Enforced by [REQ-0039](execution/rows.md#req-0039),
[REQ-0225](execution/ordering.md#req-0225),
[REQ-0480](operations/aggregation.md#req-0480) and
[REQ-0514](storage/ingestion.md#req-0514).

**P6. Values are exact.** Integers are exact 64-bit values and floats are
binary64 computed exactly as written. Nothing is rounded except where a
specification declares it. Enforced by [REQ-0016](values/numbers.md#req-0016),
[REQ-0019](values/numbers.md#req-0019),
[REQ-0438](operations/computation.md#req-0438) and
[REQ-0746](storage/csv.md#req-0746).

**P7. Vocabularies are closed.** Verbs, grammars, functions, column types,
storage profiles and conditions are closed lists. Extending one is a rule
change. Enforced by [REQ-0002](values/types.md#req-0002),
[REQ-0289](operations/expressions.md#req-0289),
[REQ-0478](operations/aggregation.md#req-0478) and
[REQ-1297](execution/lifecycle.md#req-1297).

**P8. Inheritance is deterministic.** Layers compose in one fixed order, and
the resolved specification is the same whatever spelling its layers used.
Enforced by [LAYER-3](inheritance.md#layer-3),
[LAYER-5](inheritance.md#layer-5) and [LAYER-14](inheritance.md#layer-14).

**P9. Faults are found before data.** A fault that does not depend on data
fails during validation, before any input record is read. Enforced by
[LAYER-12](inheritance.md#layer-12),
[REQ-0792](storage/resources.md#req-0792) and
[REQ-0852](storage/ingestion.md#req-0852).

**P10. Artifacts are reproducible.** The same specification and inputs produce
the same artifact bytes, and an artifact is published whole or not at all.
Enforced by [REQ-0728](storage/csv.md#req-0728),
[REQ-0752](storage/publication.md#req-0752) and
[REQ-1008](submission/define-xml.md#req-1008).

## Rule files

Language rules live in `rules/*.md`. Format and file-handling rules move to
`rules/reference/`, and the submission and project-function profiles to
`rules/profiles/`. During the 1.0.0 conversion, contracts not yet converted
keep their `REQ-NNNN` requirements and are listed with the prefix `REQ`.

| File | Prefix | Covers |
| --- | --- | --- |
| [Inheritance](inheritance.md) | LAYER | Resolve inherited layers into one minimal, ordered specification. |
| [Specification structure](specification/structure.md) | REQ | Declare identifiers, columns, derivation coverage, and source notation. |
| [Name binding](specification/binding.md) | REQ | Resolve input datasets, constructed output columns, and ODM item reads. |
| [Specification style](specification/style.md) | REQ | Fix one written form of a specification and report departures without changing its meaning. |
| [Types and conversion](values/types.md) | REQ | Define column types, missing normalization, compatibility, and conversion. |
| [Numeric values](values/numbers.md) | REQ | Define numeric representation, promotion, conversion, and overflow. |
| [Text values](values/text.md) | REQ | Define Unicode scalar identity, preservation, equality, and order. |
| [Temporal values](values/temporal.md) | REQ | Define dates, local civil datetimes, precision, canonical text, and order. |
| [Execution lifecycle](execution/lifecycle.md) | REQ | Sequence resolution, row construction, dependency evaluation, value completion, and output checks. |
| [Row construction](execution/rows.md) | REQ | Construct output rows from declared input records or groups. |
| [Ordering](execution/ordering.md) | REQ | Apply ordering terms, missing placement, stable ties, and final artifact order. |
| [Local handlers](execution/handlers.md) | REQ | Handle conditions at their expression or conversion site and report substitutions. |
| [Verification](execution/verification.md) | REQ | Apply assertions, severity, and grouped counts to completed values, and record what ran in the warning log and the verification log. |
| [Expression evaluation](operations/expressions.md) | REQ | Register and dispatch expressions, restrict nesting, and define scalar selection. |
| [Predicates](operations/predicates.md) | REQ | Evaluate the closed Boolean language using three-valued logic. |
| [Numeric computation](operations/computation.md) | REQ | Evaluate written arithmetic formulas without reassociation or presentation rounding. |
| [Aggregation](operations/aggregation.md) | REQ | Reduce eligible records in one of the three permitted key scopes. |
| [Lookup and joins](operations/lookup.md) | REQ | Match declared keys, narrow records, select a result, and answer absence. |
| [Windows](operations/windows.md) | REQ | Declare complete named windows and compute partitions, ranks, neighbors, and baseline selections. |
| [Text operations](operations/text.md) | REQ | Apply casing, inline mapping, templates, and portable regular expressions. |
| [Temporal operations](operations/temporal.md) | REQ | Compute calendar differences, study days, date completion, and precision. |
| [Project functions](operations/functions.md) | REQ | Resolve immutable runtimes and validate function inputs, results, and activation conformance. |
| [Resource resolution](storage/resources.md) | REQ | Confine declared source paths and read one immutable snapshot per physical file. |
| [Source ingestion](storage/ingestion.md) | REQ | Select input profiles and assign source field types without inferring values. |
| [CSV profile](storage/csv.md) | REQ | Read admitted CSV spellings and write canonical CSV bytes and display precision. |
| [Parquet profile](storage/parquet.md) | REQ | Read and write the closed Parquet field and value mapping. |
| [SAS source profiles](storage/sas.md) | REQ | Read SAS datasets and Transport v5/v8 through declared encoding and explicit numeric narrowing. |
| [Artifact publication](storage/publication.md) | REQ | Select artifact columns and profiles and publish complete files atomically. |
| [Submission metadata](submission/metadata.md) | REQ | Govern dataset and column metadata, origin, methods, and document references. |
| [Controlled terminology](submission/terminology.md) | REQ | Declare codelists once and validate their bindings and allowed values. |
| [Define-XML](submission/define-xml.md) | REQ | Compose study metadata into deterministic Define-XML 2.1 documents. |
| [Dataset-JSON](submission/dataset-json.md) | REQ | Write a dataset and its submission metadata as one deterministic Dataset-JSON 1.1 file. |
| [Schema language](reference/schema-language.md) | REQ | Define schema notation, registries, constraints, and canonical shorthand expansion. |

The [requirement index](https://elong0527.github.io/yamaa/reference/requirements/),
[rules at a glance](https://elong0527.github.io/yamaa/reference/rules-summary/)
and [schema fields](https://elong0527.github.io/yamaa/reference/schema-fields/)
are generated from these files and `yaml/` on every docs build. The
[glossary](reference/glossary.md) pins one meaning per shared clinical term; it
is informative. Closed grammars are defined in
[yaml/grammar/](../yaml/grammar/README.md) and rendered into their rules, and
`yaml/conditions.yaml` registers every condition an implementation may raise
with the phases that may raise it.

## Rule cards

A rule file has an `# Title`, an optional introduction, and `##` topic
headings that group cards. A card is written as:

```markdown
<a id="layer-3"></a>

### LAYER-3. Layers contribute depth first, once each, later layers taking precedence

Starting at the entry file, each layer's parents are visited left to right,
depth first, and then the layer itself. ...

Fails: `inheritance_cycle`.
Evidence: `schema-inheritance`.

For parents `A` then `B` that both inherit `Common`, ...
```

- **ID**: the file's prefix and a number. Numbers are assigned once, in the
  order cards are added, and are never reused when a card is deleted. The
  anchor is the lowercase ID.
- **Title**: one sentence without a final period that states the rule.
- **Statement**: the first paragraph, at most 90 words. It is what a reviewer
  approves.
- **Fails**: the registered conditions the rule raises, if any.
- **Evidence**: benchmarks, by directory name, and repository files such as
  tests and grammar vectors that exercise the rule.
- **Detail**: everything after those lines. Detail is normative, holds edge
  cases, tables and examples, and must agree with the statement.
- **Replaces**: during the 1.0.0 conversion only, the `REQ-NNNN` IDs the card
  absorbs, so existing citations keep resolving. It is removed before release.

Cite a rule by ID, linked to its anchor. Normative `must`, `must not`,
`should` and `may` have their usual requirement meanings. Rules state behavior,
not rationale; rationale belongs in pull requests and the documentation.

## Terminology

One term per concept across all rules, schema comments, and messages:

- **specification** -- the YAML file under review. `study` and `document` stay
  project-level (resources and submission contracts), never names for the YAML.
- **input dataset** / **output dataset** -- state the role whenever it matters.
  Bare `dataset` appears only as a key name. `table` means a documentation
  table, never data.
- **row template** -- one `rows` entry, building a group of output rows. Bare
  `template` means a string template (text operations).
- **record** -- input side (a file or input dataset row). **row** -- output side
  (an output record under construction or built).
- **column** -- dataset level. **field** -- file and schema level (ingestion and
  format profiles). **variable** -- an expression-level name (schema language).
- **path notation** -- `row.dataset`, `root.input`, `expression.source`
  (binding notation). Never `row_class.` or `root_class.`.
- **driver** -- the source whose records drive row construction: a row
  template's `dataset`, naming an input dataset or an eligible named
  intermediate ([REQ-0034](execution/rows.md#req-0034)), or root `base`
  without row templates ([REQ-0042](execution/rows.md#req-0042)). Either
  defaults to the single declared input when omitted. The term is allowed
  in normative text, schema descriptions, and user-facing messages.

A field that holds a predicate is named for how it treats the three truth
values, so one name never covers two treatments:

| Field | Treatment |
| --- | --- |
| `filter` | Keeps the rows or records whose result is `TRUE`. |
| `when` | `TRUE` selects a branch, row, or group; `FALSE` and `UNKNOWN` are treated alike and select nothing (`case`, `assert`, `row_count`). |
| `require` | The result must be `TRUE`; `FALSE` and `UNKNOWN` fail (`assert`). |
| `condition` | Each of `TRUE`, `FALSE`, and `UNKNOWN` has its own result (`flag`). |

`expr` holds an expression in a closed text grammar that produces a value
(`compute`, `aggregate`), never a predicate. `then` and `otherwise` hold
values (`case`), never predicates.

## Changing a rule

- A change to a card's title or statement changes the language and needs the
  maintainer's approval. A change to detail, evidence or tests needs ordinary
  review.
- New behavior needs a card with evidence. A failure needs a registered
  condition named in `Fails`.
- An editorial change keeps behavior and failure conditions; a semantic change
  needs its own conformance evidence.
- Behavior the rules do not specify stays an open question in an issue. An
  implementation's behavior is never the specification.
