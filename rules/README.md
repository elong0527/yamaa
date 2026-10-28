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
Enforced by [SPEC-4](specification.md#spec-4),
[INPUT-5](inputs.md#input-5),
[OUTPUT-5](output.md#output-5) and
[INPUT-1](inputs.md#input-1).

**P2. Values never convert silently.** An operation never converts its inputs.
A value converts once, to its column's declared type, by one table, and a
failed conversion is a named failure. Enforced by
[VALUE-2](values.md#value-2), [VALUE-3](values.md#value-3) and
[INPUT-7](inputs.md#input-7).

**P3. Missing is a value.** Missing is distinct from every string, number and
the empty string; no text spells it, every type admits it, and every operation
states what it does with it. Enforced by
[VALUE-25](values.md#value-25),
[FORMULA-4](formulas.md#formula-4),
[COLUMN-10](columns.md#column-10),
[FORMULA-24](formulas.md#formula-24) and
[INPUT-8](inputs.md#input-8).

**P4. Ambiguity fails.** Two values for one key, several matching records
without a declared selection, an unmapped value or text that does not parse
fails with a named condition instead of choosing. Enforced by
[COLUMN-2](columns.md#column-2),
[LOOKUP-10](lookup.md#lookup-10),
[FORMULA-20](formulas.md#formula-20) and
[NAME-13](names.md#name-13).

**P5. Order is data.** Stored record order, construction order and declared
order terms decide every tie. No operation reorders records or chooses an
order of its own. Enforced by [ROW-7](rows.md#row-7),
[VALUE-23](values.md#value-23),
[FORMULA-19](formulas.md#formula-19) and
[INPUT-2](inputs.md#input-2).

**P6. Values are exact.** Integers are exact 64-bit values and floats are
binary64 computed exactly as written. Nothing is rounded except where a
specification declares it. Enforced by [VALUE-7](values.md#value-7),
[VALUE-9](values.md#value-9),
[FORMULA-26](formulas.md#formula-26) and
[CSV-8](reference/csv.md#csv-8).

**P7. Vocabularies are closed.** Verbs, grammars, functions, column types,
storage profiles and conditions are closed lists. Extending one is a rule
change. Enforced by [VALUE-1](values.md#value-1),
[EXPR-1](expressions.md#expr-1),
[FORMULA-18](formulas.md#formula-18) and
[COLUMN-15](columns.md#column-15).

**P8. Inheritance is deterministic.** Layers compose in one fixed order, and
the resolved specification is the same whatever spelling its layers used.
Enforced by [LAYER-3](inheritance.md#layer-3),
[LAYER-5](inheritance.md#layer-5) and [LAYER-14](inheritance.md#layer-14).

**P9. Faults are found before data.** A fault that does not depend on data
fails during validation, before any input record is read. Enforced by
[LAYER-12](inheritance.md#layer-12),
[FILE-11](reference/files.md#file-11),
[INPUT-1](inputs.md#input-1) and
[VALUE-24](values.md#value-24).

**P10. Artifacts are reproducible.** The same specification and inputs produce
the same artifact bytes, and an artifact is published whole or not at all.
Enforced by [CSV-3](reference/csv.md#csv-3),
[OUTPUT-8](output.md#output-8) and
[DEFINE-23](profiles/define-xml.md#define-23).

**P11. A package is consistent and traceable.** The datasets, transport files,
run reports and Define-XML documents a project publishes come from one set of
resolved specifications and agree on every fact they share. Every published
fact names its source: an identifier derives from declared identifiers,
metadata from the specification that built the data, and an origin is
declared, never inferred. Enforced by
[JSON-5](profiles/dataset-json.md#json-5),
[JSON-6](profiles/dataset-json.md#json-6),
[DEFINE-9](profiles/define-xml.md#define-9),
[SUB-19](profiles/submission.md#sub-19) and
[INPUT-11](inputs.md#input-11).

Historical exports ([DEFINE-30](profiles/define-xml.md#define-30)) are records
of received declarations rather than assertions about current derived
artifacts. They never publish transport files or claim submission context.

## Rule files

Language rules live in `rules/*.md`, format and file-handling rules in
`rules/reference/`, and the submission, project-file and project-function
profiles in `rules/profiles/`.

| File | Prefix | Covers |
| --- | --- | --- |
| [Specification structure](specification.md) | SPEC | Declare root fields, unique identifiers, column derivation coverage, internal and copied columns, and ASCII source. |
| [Inheritance](inheritance.md) | LAYER | Resolve inherited layers into one minimal, ordered specification. |
| [Name binding](names.md) | NAME | Resolve input datasets, qualified and bare names, driver reads, and ODM item reads. |
| [Values](values.md) | VALUE | Define column types, missing values, numbers, text, dates and datetimes, conversion, and order. |
| [Inputs](inputs.md) | INPUT | Select input profiles, keep stored record order, and type source fields without inferring values. |
| [Row construction](rows.md) | ROW | Construct output rows from declared input records or groups, or from the distinct keys of the base records. |
| [Column derivation](columns.md) | COLUMN | Sequence a run, order column dependencies, complete each value's lifecycle, handle conditions at their site, and name the phase and rule each condition reports. |
| [Expressions](expressions.md) | EXPR | Register and dispatch expressions, give each one result type, restrict nesting, and copy, select and classify values. |
| [Formulas](formulas.md) | FORMULA | Read the one closed expression language in its predicate, aggregate and expression fields, and evaluate it exactly as written. |
| [Lookup and joins](lookup.md) | LOOKUP | Match declared keys, narrow and select records, answer absence, and scope aggregates to one relation. |
| [Windows](windows.md) | WINDOW | Declare complete named windows and compute partitions, ranks, neighbors, carried values, and baseline flags. |
| [Text](text.md) | TEXT | Pad, take pieces by position, count scalars, write numbers with fixed decimals, change case, interpolate templates, and extract and replace with portable regular expressions. |
| [Dates and times](temporal.md) | TIME | Compute differences, study days and times of day, read declared date forms, assemble and complete collected dates and times, report collected precision and imputation flags, and apply civil arithmetic. |
| [Verification](verification.md) | CHECK | Apply assertions, severity, and grouped counts to completed values, and record what each run read, counted and checked in one run report. |
| [Output](output.md) | OUTPUT | Select artifact columns, keys, profiles, and row order, and publish complete files atomically. |
| [File resolution](reference/files.md) | FILE | Resolve declared local paths from their base, reach one regular file, and read one immutable snapshot per physical file. |
| [CSV profile](reference/csv.md) | CSV | Read admitted CSV spellings and write canonical CSV bytes and display precision. |
| [Parquet profile](reference/parquet.md) | PARQUET | Read and write the closed Parquet field and value mapping. |
| [SAS source profiles](reference/sas.md) | SAS | Read SAS datasets and Transport v5/v8 through declared encoding and explicit numeric narrowing. |
| [ODM source profile](reference/odm.md) | ODM | Read an ODM 1.3 or 2.0 snapshot into the item table, refusing transactional content and documents without clinical data, and skip audit records. |
| [Dataset-JSON source profiles](reference/dataset-json.md) | DSJSON | Read a Dataset-JSON 1.1 file in either representation, typing each field by its column's dataType and narrowing only by declaration. |
| [Schema language](reference/schema-language.md) | SCHEMA | Define schema notation, registries, constraints, canonical shorthand expansion, and the one spelling of a written value. |
| [Submission metadata](profiles/submission.md) | SUB | Govern dataset and column metadata, origin, methods, and document references. |
| [Controlled terminology](profiles/terminology.md) | CT | Declare codelists, compose them by `id` across their sources, and enforce the items each binding lists. |
| [Define-XML](profiles/define-xml.md) | DEFINE | Compose study metadata into deterministic Define-XML 2.1 documents. |
| [Dataset-JSON](profiles/dataset-json.md) | JSON | Write a dataset and its submission metadata as one deterministic Dataset-JSON 1.1 file. |
| [SAS Transport](profiles/xpt.md) | XPT | Write a dataset as one deterministic SAS Transport version 5 file in the public record layout. |
| [Project functions](profiles/functions.md) | FUNC | Define each project function once in a function file, resolve immutable runtimes, and validate function inputs, results, and activation conformance. |
| [Project file](profiles/project.md) | PROJECT | List a study's function files and host locks, declare its terminology and submission sections once, compose a project file on one parent, and check, run and package the specifications its sections list. |

The [rule index](https://elong0527.github.io/yamaa/reference/rule-index/),
[rules at a glance](https://elong0527.github.io/yamaa/reference/rules-summary/)
and [schema fields](https://elong0527.github.io/yamaa/reference/schema-fields/)
are generated from these files and `yaml/` on every docs build. The
[glossary](reference/glossary.md) pins one meaning per shared clinical term; it
is informative. Closed grammars are defined in
[yaml/grammar/](../yaml/grammar/README.md) and rendered into their rules, and
`yaml/conditions.yaml` registers every condition an implementation may raise
with the phases that may raise it and the message its issues carry. The
tables of [SUB-3](profiles/submission.md#sub-3),
[SUB-6](profiles/submission.md#sub-6),
[SUB-9](profiles/submission.md#sub-9),
[SUB-10](profiles/submission.md#sub-10),
[SUB-15](profiles/submission.md#sub-15),
[SUB-26](profiles/submission.md#sub-26) and
[JSON-10](profiles/dataset-json.md#json-10), and the lists of
[SUB-6](profiles/submission.md#sub-6),
[SUB-10](profiles/submission.md#sub-10),
[SUB-14](profiles/submission.md#sub-14),
[CT-2](profiles/terminology.md#ct-2) and
[DEFINE-6](profiles/define-xml.md#define-6), list values of the closed CDISC
vocabularies, which `yaml/vocabulary/cdisc.yaml` holds, and repository
checks keep them to that file.

## Rule cards

A rule file has an `# Title`, an optional introduction, and `##` topic
headings that group cards. A card is written as:

```markdown
<a id="layer-3"></a>

### LAYER-3. Layers contribute from the root ancestor to the entry, each after its fragments, later layers taking precedence

Starting at the entry file, each layer's `parent` names the next layer up the
chain, which ends at a layer without a parent. ...

Fails: `inheritance_cycle`, `duplicate_layer`.
Evidence: `schema-inheritance`.

For a study entry whose parent is a compound layer whose parent is an ...
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

Cite a rule by ID, linked to its anchor. Normative `must`, `must not`,
`should` and `may` have their usual requirement meanings. Rules state behavior,
not rationale; rationale belongs in pull requests and the documentation.

## Terminology

One term per concept across all rules, schema comments, and messages:

- **specification** -- the YAML file under review. `study` and `document` stay
  project-level (resources and submission contracts), never names for the YAML.
- **project file** -- the one study-level YAML a caller passes: it lists the
  study's **function files**, each defining one project function with a
  callable for each host language it implements, holds each host language's
  lock, and declares the study's terminology and submission sections
  ([Project file](profiles/project.md)).
- **input dataset** / **output dataset** -- state the role whenever it matters.
  Bare `dataset` appears only as a key name. `table` means a documentation
  table, never data.
- **row template** -- one `rows` entry, building a group of output rows. Bare
  `template` means a string template (text operations).
- **fragment** -- a specification file that an `include` entry names
  ([LAYER-3](inheritance.md#layer-3)), or that a check composes on a parent
  the caller names ([LAYER-4](inheritance.md#layer-4)); it declares neither
  `parent` nor `include`. A layer that is not complete on its own is a partial
  specification, never a fragment.
- **record** -- input side (a file or input dataset row). **row** -- output side
  (an output record under construction or built).
- **column** -- dataset level. **field** -- file and schema level (ingestion and
  format profiles). **variable** -- an expression-level name (schema language).
- **path notation** -- `row.dataset`, `root.input`, `expression.source`
  (binding notation). Never `row_class.` or `root_class.`.
- **driver** -- the source whose records drive row construction: a row
  template's `dataset`, naming an input dataset, an eligible named
  intermediate or a pair relation ([ROW-1](rows.md#row-1),
  [LOOKUP-27](lookup.md#lookup-27)), or root `base`
  without row templates ([ROW-8](rows.md#row-8)). Either
  defaults to the single declared input when omitted. The term is allowed
  in normative text, schema descriptions, and user-facing messages.

A field that holds a predicate is named for how it treats the three truth
values, so one name never covers two treatments:

| Field | Treatment |
| --- | --- |
| `filter` | Keeps the rows or records whose result is `TRUE`. |
| `when` | `TRUE` selects a branch, row, or group; `FALSE` and `UNKNOWN` are treated alike and select nothing (`case`, `assert`, `not_missing`, `row_count`). |
| `require` | The result must be `TRUE`; `FALSE` and `UNKNOWN` fail (`assert`). |
| `condition` | Each of `TRUE`, `FALSE`, and `UNKNOWN` has its own result (`flag`). |

`expr` holds an expression in a closed text grammar that produces a value
(`aggregate`), never a predicate. `then` and `otherwise` hold
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
