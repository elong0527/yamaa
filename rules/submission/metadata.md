---
id: submission/metadata
title: Submission metadata
status: normative
---

# Submission metadata

## Purpose

Govern dataset and column metadata, origin, methods, and document references.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Verification](../execution/verification.md).
- [Specification structure](../specification/structure.md).
- [CSV profile](../storage/csv.md).
- [Define-XML](define-xml.md).
- [Controlled terminology](terminology.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).

## Requirements

### Where it is declared

<a id="req-0855"></a>

**REQ-0855.** `root.submission` carries the dataset's metadata and
`column.submission` carries one column's. Both are optional in the schema. A specification that no submission
document represents needs neither.
[Define-XML](define-xml.md) requires them from a specification that joins a document.

<a id="req-0856"></a>

**REQ-0856.** Submission metadata is declared only for a column `output.columns`
carries. An internal column is not in the artifact, so it is not in the
document that describes the artifact, and declaring metadata for an internal
column is an error rather than an ignored declaration.

<a id="req-0857"></a>

**REQ-0857.** The free-form `metadata` map is uninterpreted annotation; no field
below reads the map. A study moves governed facts from the map to submission
metadata. The map and submission metadata are not merged. The map is never a
fallback.

<a id="req-0858"></a>

**REQ-0858.** The map must not carry a key this contract governs. Root `metadata`
rejects all dataset metadata field names above. A column's `metadata` rejects
all column metadata field names below.

### Standard families

<a id="req-0859"></a>

**REQ-0859.** Every dataset follows one foundational standard, and the study
document binds it: [REQ-0965](define-xml.md#req-0965) resolves each dataset to a declared standard of type
`IG`. A specification does not name the standard. The implementation-guide
release changes between submissions. Binding the release in the specification
would version every dataset specification against one study. This contract reads
the bound standard's published name as one of three closed **families**:

| Family | Standard names |
|---|---|
| `sdtm` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD` |
| `send` | `SENDIG`, `SENDIG-AR`, `SENDIG-DART`, `SENDIG-GENETOX` |
| `adam` | `ADaMIG`, `ADaMIG-MD` |

<a id="req-0860"></a>

**REQ-0860.** The table is closed. A dataset bound to an `IG` standard outside
the table fails: the origin pairs, the core mapping, and the domain and
purpose decisions below are all family-specific, and this design closes none
of them for a standard it does not name. `BIMO` is excluded today; admitting
it changes this table rather than an implementation's judgment.

<a id="req-0861"></a>

**REQ-0861.** A dataset must not be bound to a standard of type `CT`.
Controlled terminology qualifies a codelist and not a dataset; [Controlled terminology](terminology.md) owns that
binding.

### Dataset metadata

<a id="req-0862"></a>

**REQ-0862.** `label` states what the dataset is, and is required. It is the
dataset's own description and is distinct from `domain`: `DM` is the domain
and `Demographics` is the label.

<a id="req-0863"></a>

**REQ-0863.** `class` names the general observation class and is required, in
the exact spelling and case the vocabulary publishes. `subclass` is declared
only when the dataset's implementation guide defines one for it.

<a id="req-0864"></a>

**REQ-0864.** `structure` states in prose the level of detail one record
represents, such as `One record per subject`. `structure` is required.
Keys alone do not fix a dataset's structure: one key list serves
several structures.

<a id="req-0865"></a>

**REQ-0865.** `repeating` is required and states whether the dataset may carry
more than one record per subject or pool. `reference_data` defaults to false
and states whether the dataset carries reference data rather than subject
data. A dataset declaring `reference_data: true` must declare
`repeating: false`.

<a id="req-0866"></a>

**REQ-0866.** `domain` names the domain the dataset belongs to. It defaults to
the specification's `domain`, so an ordinary dataset states the domain once.
A split or supplemental dataset declares the domain. The dataset's
own name is not its parent domain.

<a id="req-0867"></a>

**REQ-0867.** `domain` is declared only for a dataset whose family is `sdtm` or
`send`. The `adam` family has no domain, so declaring a domain there is an
error rather than an unused value.

### Column metadata

<a id="req-0868"></a>

**REQ-0868.** `data_type` states the type the submission represents the column
as. `data_type` defaults from the declared column type, and each declared
type admits a closed set:

| Declared type | Default | Also admits |
|---|---|---|
| `str` | `text` | types listed below |
| `int` | `integer` | none |
| `float` | `float` | none |
| `date` | `date` | none |
| `datetime` | `datetime` | none |

`str` admits `date`, `datetime`, `time`, `partialDate`, `partialTime`,
`partialDatetime`, `incompleteDate`, `incompleteTime`, `incompleteDatetime`,
`durationDatetime`, `intervalDatetime`, and `URI`.

<a id="req-0869"></a>

**REQ-0869.** A `str` column admits temporal submission types. A
submission carries a partial or incomplete date as text. [Temporal values](../values/temporal.md) does not admit
partial or incomplete dates as `date` or `datetime`. The two temporal column
types admit only their matching submission type. [Temporal values](../values/temporal.md) defines each
temporal type as a complete value, and the submission types of the same name
mean the same thing. A number's submission type is fixed: [Types and conversion](../values/types.md) already
decides integer or binary64.

<a id="req-0870"></a>

**REQ-0870.** `data_type` never changes a value. `data_type` states how a value
[Types and conversion](../values/types.md) already typed appears in the document, and no derivation, verification,
key, order term, or artifact byte can observe it.

#### Length and significant digits

<a id="req-0871"></a>

**REQ-0871.** `length` is the maximum expected value length, as the submission
standard defines the term: a property of the column declaration, not of the
data one run produced. The length is a positive integer.

<a id="req-0872"></a>

**REQ-0872.** `length` is required when `data_type` resolves to `text`,
`integer`, or `float` unless [REQ-0875](metadata.md#req-0875) derives it, and must not be declared
otherwise. The other submission types carry fixed-form values, so a length
for such a type restates that form.

<a id="req-0873"></a>

**REQ-0873.** `significant_digits` is required when `data_type` resolves to
`float` and must not be declared otherwise. The field counts digits after
the decimal point and is a non-negative integer.

<a id="req-0874"></a>

**REQ-0874.** A `length` on a `str` column is enforced. The column's completed
values must contain at most `length` [Text values](../values/text.md) scalar values, as [Verification](../execution/verification.md)'s
`max_length` requires, so the declared length binds rather than describes.

<a id="req-0875"></a>

**REQ-0875.** On a `str` column with a `max_length` verification, `length` is
derived from the verification's `max` and need not be declared. `length` and
`max_length` state one bound, so stating both creates disagreement. Declaring
both is accepted when equal and rejected when different. The bound is checked
once either way.

<a id="req-0876"></a>

**REQ-0876.** A `length` declared on any other column type is not enforced.
[Verification](../execution/verification.md) states why: rendered text is a property of [Types and conversion](../values/types.md)'s and [CSV profile](../storage/csv.md)'s rendering
rather than of the value, so a length over rendered text would assert
something this language does not decide. The declaration
is carried into the document unchecked, and this contract states that openly.
Enforcing a rendered width changes both [Verification](../execution/verification.md)'s `max_length` and this
requirement.

#### Display format and role

<a id="req-0877"></a>

**REQ-0877.** `display_format` is presentation text carried for a reader.
`display_format` never changes a computed value, a rendered artifact value,
or [CSV profile](../storage/csv.md)'s `output.decimals`. A study that wants different artifact digits
declares `output.decimals`, which [CSV profile](../storage/csv.md) owns. The two settings are
independent, and this contract does not reconcile them: one is a display
a document reports and the other is a display an artifact carries.

<a id="req-0878"></a>

**REQ-0878.** `role` states how the column is used within its dataset, in the
vocabulary the dataset's standard publishes, which this language does not
close: each implementation guide publishes and versions its own.

<a id="req-0879"></a>

**REQ-0879.** `role` must not be declared for a dataset whose family is `adam`.
That family defines no role vocabulary, so a role in that family would name
nothing.

#### Core and mandatory

<a id="req-0880"></a>

**REQ-0880.** `core` is the standard's core designation: `Req` for required,
`Exp` for expected, or `Perm` for permissible, declared for a dataset whose
family is `sdtm` or `send` and never for `adam`.

<a id="req-0881"></a>

**REQ-0881.** `mandatory` states whether the completed column must carry a
value. `mandatory` is not a second spelling of `core`, and the mapping
between them is standard-specific:

<a id="req-0882"></a>

**REQ-0882.** For `sdtm` and `send`, `core` is required and `mandatory`
  defaults from it: `Req` gives true, and `Exp` and `Perm` give false.

<a id="req-0883"></a>

**REQ-0883.** For `sdtm` and `send`, a declared `mandatory: true` on an `Exp`
  or `Perm` column is an accepted sponsor restriction. A declared
  `mandatory: false` on a `Req` column is rejected: the standard requires the
  column, and a specification cannot relax its standard by declaration.

<a id="req-0884"></a>

**REQ-0884.** For `adam`, `mandatory` is required and nothing derives it.

<a id="req-0885"></a>

**REQ-0885.** A column declaring `mandatory: true` must also carry a
`not_missing` verification or appear in `keys`, whose non-missing
requirement [Artifact publication](../storage/publication.md) already states. A document that asserts a value is always
present, over an artifact nothing checks, asserts something no run proves.

#### Codelist

<a id="req-0886"></a>

**REQ-0886.** `codelist` names a codelist the study document declares. [Controlled terminology](terminology.md)
owns that object, the values a binding enforces, and its agreement with an
`allowed_values` verification.

### Value-level metadata

<a id="req-1162"></a>

**REQ-1162.** A `rows` entry may carry a `submission` map keyed by column name.
Each entry declares the submission metadata for one value of the row's
discriminator (see [REQ-1163](metadata.md#req-1163)): the column-level `submission` stays the shared
declaration for the column, and the row-level entry supplies per-value
overrides, so a findings domain can give each `--TESTCD` its own codelist,
origin, or length without repeating the shared metadata. Fields the
row-level entry leaves absent inherit from the column-level declaration.

<a id="req-1163"></a>

**REQ-1163.** Value-level metadata hangs on the domain's `--TESTCD` column:
the domain must declare a `<DOMAIN>TESTCD` column. A row template carrying
`submission` without that column declared has no discriminator to hang
values on, and the declaration is an error.

<a id="req-1164"></a>

**REQ-1164.** The discriminator must be a declared literal: the row template
derives `<DOMAIN>TESTCD` as `{literal: <code>}` with a string code. A dynamic
expression leaves the value unknown until data arrives, and value-level
metadata is validated without data (see [REQ-0907](metadata.md#req-0907)), so a dynamic
discriminator fails loudly rather than silently attaching metadata to an
unknown value.

<a id="req-1165"></a>

**REQ-1165.** Each `submission` key must name a declared column the row
template or the column declaration derives. A key naming anything else is
an error, not an ignored declaration.

<a id="req-1166"></a>

**REQ-1166.** [REQ-0856](metadata.md#req-0856) applies per value: the column must be in
`output.columns`. Metadata for an internal column is an error at row level
exactly as it is at column level.

<a id="req-1167"></a>

**REQ-1167.** Two row templates may declare submission metadata for the same
(column, test code) pair only with identical declarations: identical
declarations describe one value, and conflicting declarations are an error,
not a last-wins merge.

<a id="req-1168"></a>

**REQ-1168.** [REQ-0897](metadata.md#req-0897) through [REQ-0899](metadata.md#req-0899) apply to each value-level
entry against the derivation that produces the column's value in that row
template (the row template's derivation, or the column-level derivation
when the row template inherits a uniform one): the refutation follows the
entry. The origin claim is per value.

### Origin

<a id="req-0887"></a>

**REQ-0887.** `origin` is required for every column carrying submission
metadata. A submission document states where every value came from, and a
column with no declared origin has no place in a submission document.

<a id="req-0888"></a>

**REQ-0888.** `origin.type` is one of `Assigned`, `Collected`, `Derived`,
`Not Available`, `Other`, `Predecessor`, or `Protocol`. `origin.source` is one
of `Investigator`, `Sponsor`, `Subject`, or `Vendor`. Both vocabularies are
closed and neither is extended by a specification.

<a id="req-0889"></a>

**REQ-0889.** Which pairs are admitted depends on the family. For `sdtm`:

| Type | Admitted sources |
|---|---|
| `Collected` | `Subject`, `Investigator`, `Vendor` |
| `Derived` | `Vendor`, `Sponsor` |
| `Assigned` | `Vendor`, `Sponsor` |
| `Protocol` | `Sponsor` |
| `Other` | any |
| `Predecessor` | none; `source` must be absent |
| `Not Available` | not admitted |

<a id="req-0890"></a>

**REQ-0890.** For `adam`, the admitted types are `Derived`, `Assigned`,
`Predecessor`, and `Other`, and `source` is derived rather than declared:
`Sponsor` for `Derived`, `Assigned`, and `Other`, and absent for
`Predecessor`. A declared `source` is rejected. `Collected` and `Protocol`
are not admitted.

<a id="req-0891"></a>

**REQ-0891.** For `send`, every type is admitted and `source` must be absent.
That family identifies responsibility through its own trial-summary and
findings columns rather than through this attribute.

<a id="req-0892"></a>

**REQ-0892.** `origin.description` is required when `type` is `Predecessor`,
and names the dataset and column the value was copied from in the form
`<dataset>.<column>`. The description is also required when `type` is `Other`
or `Not Available`. Neither states anything alone. The
description is optional elsewhere.

<a id="req-0893"></a>

**REQ-0893.** A value collected from a person is traceable to the form it was
collected on. A column with a `Collected` origin from `Investigator` or
`Subject` therefore references the study's annotated case report form, and
[Define-XML](define-xml.md) rejects a reference naming any other document.

<a id="req-0894"></a>

**REQ-0894.** The reference is derived when `documents` is omitted. The
annotated case report form is the only document the reference can name. A
column declares `documents` to say where in the form the value was collected.
Omitting `documents` leaves the reference without a page.

<a id="req-0895"></a>

**REQ-0895.** A column whose `origin.type` is `Derived` must declare `method`.
The algorithm is what a derived origin claims exists; a claim with no
algorithm attached is not traceable.

#### What the graph refutes

<a id="req-0896"></a>

**REQ-0896.** The [Execution lifecycle](../execution/lifecycle.md) derivation graph proves some facts about a column value.
A declared origin that contradicts a proven fact is rejected. The graph never
supplies an origin. The graph cannot tell an investigator-recorded
value from a vendor-transmitted or protocol-fixed value. Guessing would put an
unverifiable claim in a submission document. Origin is always declared and
sometimes refuted, never inferred.

<a id="req-0897"></a>

**REQ-0897.** A column whose derivation is anything other than a bare `source`
or a bare `literal` computes its value from other values. The column's
`origin.type` must be `Derived`, `Assigned`, or `Other`. `Collected`,
`Protocol`, `Predecessor`, and `Not Available` are refuted: each asserts
the value arrived as it stands, and the specification shows it did not.

<a id="req-0898"></a>

**REQ-0898.** A column derived by a bare `literal` takes the same value in
every row regardless of any input. The column's `origin.type` must be
`Assigned`, `Protocol`, or `Other`. `Collected`, `Derived`, `Predecessor`,
and `Not Available` are refuted: nothing was observed, computed, or copied,
and a value the specification states outright is available.

<a id="req-0899"></a>

**REQ-0899.** A column derived by a bare `source` copies one value of one
declared dataset. The column's `origin.type` must not be `Derived`: nothing
was computed. Every remaining type stays admissible. Whether that
stored value was collected, assigned, fixed by the protocol, or copied from
a predecessor dataset is a fact about the source and not about this
specification.

<a id="req-0900"></a>

**REQ-0900.** For a row-derived column, apply [REQ-0897](metadata.md#req-0897) through [REQ-0899](metadata.md#req-0899) to the
derivation each `rows` entry declares. If entries disagree, the column keeps
only the types all entries admit. No shared type is an error, not a silently
admitted origin.

<a id="req-1157"></a>

**REQ-1157.** A column whose `origin.type` is `Predecessor` copies its values
unchanged from the predecessor dataset and column `origin.description` names.
The column's derivation must therefore be a bare `source` of that dataset and
column: any other derivation computes rather than copies, and the
[REQ-0897](metadata.md#req-0897) refutation already rejects a computed column that claims
otherwise. When both the column and the named predecessor column declare
submission metadata, their declared labels must be equal. A copy keeps the
name, the label, and the values of what it copies.

### Method

<a id="req-0901"></a>

**REQ-0901.** `method` states the algorithm that produces a column's value.
Written as a string, `method` is the algorithm's prose description; written
as a mapping, `method` adds a name, a kind, machine-readable code, and
supporting documents.

<a id="req-0902"></a>

**REQ-0902.** `description` is required. `type` defaults to `Computation`; a
method that replaces a missing value with a substitute declares `Imputation`.

<a id="req-0903"></a>

**REQ-0903.** `expression` carries code as text under a named `context`. This
language neither parses nor evaluates the code, and a generated document
carries the code unchanged. No rule checks the code against the
specification's derivation. An expression is sponsor-supplied documentation,
not a second definition of the column.

<a id="req-0904"></a>

**REQ-0904.** No `expression` is generated from a derivation. A derivation has
no canonical written form in this language, and inventing a form here would
fix a printing of every expression the registry holds without a rule that
owns the printing. A future rule may define the printing and generate the
expression from it; until a rule does, the field is declared or absent.

### Comment and documents

<a id="req-0905"></a>

**REQ-0905.** `comment` carries a sponsor note about a dataset or a column.
Written as a string, `comment` is the note's text; written as a mapping,
`comment` adds the documents the note refers to. Comments are declared where
they are used, and one comment is not shared between two datasets or two
columns. [Define-XML](define-xml.md) mints each comment an identifier from the place it is declared,
so a shared comment would need an identifier space this design does not open.

<a id="req-0906"></a>

**REQ-0906.** A document reference names a document the study document
declares, and optionally the place within it. As a string, the reference
is the identifier; as a mapping, the reference adds `pages`. A `pages`
reference states whether the `refs` are physical page numbers or named
destinations and carries the `refs` as a space-separated list.

### What a specification alone validates

<a id="req-0907"></a>

**REQ-0907.** Every requirement above that does not depend on a family is
checked when the specification is validated on its own: the vocabularies, the
required and prohibited field combinations, the length binding in [REQ-0874](metadata.md#req-0874), the
graph refutations in [REQ-0896](metadata.md#req-0896) through [REQ-0900](metadata.md#req-0900), the value-level
declaration rules in [REQ-1162](metadata.md#req-1162) through [REQ-1168](metadata.md#req-1168), and the presence of `method` for
a declared `Derived` origin.

<a id="req-0908"></a>

**REQ-0908.** Every family-dependent requirement is checked when [Define-XML](define-xml.md) composes
the specification into a document. Only the study document says which
standard the identifier names. A specification that is never composed carries
declarations no family has judged. The specification is not yet part of
a submission.

### Interface behavior

<a id="req-1130"></a>

**REQ-1130.** The `submission_dataset_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_dataset_class.label` | Description of the dataset as a whole, distinct from the domain it derives. |
| `submission_dataset_class.class` | General observation class of the dataset. |
| `submission_dataset_class.subclass` | Observation subclass, declared when the standard defines one for the dataset. |
| `submission_dataset_class.structure` | Level of detail one record represents, stated in prose. |
| `submission_dataset_class.repeating` | Whether the dataset may carry more than one record per subject or pool. |
| `submission_dataset_class.reference_data` | Whether the dataset carries reference data rather than subject data. |
| `submission_dataset_class.domain` | Domain the dataset belongs to; defaults to the specification's domain. |
| `submission_dataset_class.comment` | Sponsor comment carried with the dataset definition. |

<a id="req-1131"></a>

**REQ-1131.** The `submission_column_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_column_class.core` | Standard core designation; [Submission metadata](metadata.md) states which family declares it and what it derives. |
| `submission_column_class.mandatory` | Whether the column must carry a value; [Submission metadata](metadata.md) states when it is derived and when declared. |
| `submission_column_class.role` | Role the column plays in its dataset, in the vocabulary its standard publishes. |
| `submission_column_class.data_type` | Submission data type; [Submission metadata](metadata.md) defaults it from the declared column type. |
| `submission_column_class.length` | Maximum expected value length; [Submission metadata](metadata.md) derives it from a max_length verification and binds it to the constraint that enforces it. |
| `submission_column_class.significant_digits` | Digits after the decimal point; required with a float submission data type. |
| `submission_column_class.display_format` | Presentation format carried for a reader; it never changes a computed value. |
| `submission_column_class.codelist` | Codelist whose values this column carries; [Controlled terminology](terminology.md) defines the binding. |
| `submission_column_class.origin` | Declared provenance of the value; [Submission metadata](metadata.md) states what the graph refutes. |
| `submission_column_class.method` | Algorithm producing the value; [Submission metadata](metadata.md) requires it for a derived origin. |
| `submission_column_class.comment` | Sponsor comment carried with the column definition. |

<a id="req-1132"></a>

**REQ-1132.** The `submission_origin_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_origin_class.type` | How the value originated. |
| `submission_origin_class.source` | Party responsible for the value; [Submission metadata](metadata.md) closes the pairs each family admits and derives it where one fixes it. |
| `submission_origin_class.description` | Explanatory text, required where [Submission metadata](metadata.md) states the origin is incomplete without it. |
| `submission_origin_class.documents` | Supporting documents evidencing the origin; [Submission metadata](metadata.md) derives the annotated case report form reference when a collected origin omits them. |

<a id="req-1133"></a>

**REQ-1133.** The `submission_method_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_method_class.name` | Name of the algorithm; [Define-XML](define-xml.md) derives one when it is omitted. |
| `submission_method_class.type` | Kind of algorithm the method describes. |
| `submission_method_class.description` | Statement of the algorithm in prose. |
| `submission_method_class.expression` | Machine-readable code the method is also stated in. |
| `submission_method_class.documents` | Supporting documents carrying the full algorithm. |

<a id="req-1134"></a>

**REQ-1134.** The `submission_comment_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_comment_class.text` | Text of the comment. |
| `submission_comment_class.documents` | Supporting documents the comment refers to. |

<a id="req-1135"></a>

**REQ-1135.** The `formal_expression_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `formal_expression_class.context` | Language the code is written in, named for a reader that evaluates it. |
| `formal_expression_class.code` | The code itself; nothing in this language parses or executes it. |

<a id="req-1136"></a>

**REQ-1136.** The `document_reference_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `document_reference_class.document` | Supporting document this reference names. |
| `document_reference_class.pages` | Place within the document the reference points at. |

<a id="req-1137"></a>

**REQ-1137.** The `page_reference_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `page_reference_class.type` | Whether refs are physical page numbers or named destinations. |
| `page_reference_class.refs` | Space-separated page numbers or destination names. |
| `page_reference_class.title` | Label a reader sees in place of the raw reference. |

<a id="req-1138"></a>

**REQ-1138.** The `submission_comment` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_comment` | Comment text, or text with the documents it refers to. |

<a id="req-1139"></a>

**REQ-1139.** The `submission_method` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `submission_method` | Algorithm stated in prose, or with its name, code, and documents. |

<a id="req-1140"></a>

**REQ-1140.** The `document_reference` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `document_reference` | Document identifier, or an identifier with the place it points at. |

<a id="req-1141"></a>

**REQ-1141.** The `core_designation` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `core_designation` | Required, expected, or permissible in the dataset's standard. |

<a id="req-1142"></a>

**REQ-1142.** The `origin_type` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `origin_type` | Define-XML 2.1 origin type. |

<a id="req-1143"></a>

**REQ-1143.** The `origin_source` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `origin_source` | Define-XML 2.1 origin source. |

<a id="req-1144"></a>

**REQ-1144.** The `method_type` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `method_type` | Kind of algorithm a method describes. |

<a id="req-1145"></a>

**REQ-1145.** The `define_data_type` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_data_type` | Submission data type a column is represented by; [Submission metadata](metadata.md) closes which ones each declared column type admits. |

<a id="req-1146"></a>

**REQ-1146.** The `dataset_class_name` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `dataset_class_name` | General observation class, in the exact case Define-XML 2.1 publishes. |

<a id="req-1147"></a>

**REQ-1147.** The `dataset_subclass_name` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `dataset_subclass_name` | Observation subclass, in the exact case Define-XML 2.1 publishes. |

<a id="req-1148"></a>

**REQ-1148.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | Governed submission metadata. [Submission metadata](metadata.md) defines what each field means, which combinations a standard admits, and what the derivation graph refutes. [Controlled terminology](terminology.md) owns the codelist a binding names, and [Define-XML](define-xml.md) composes these declarations with a study document into one Define-XML 2.1 document. Every identifier here names an object the study document declares. A specification validates on its own with the names unresolved; [Define-XML](define-xml.md) resolves them when the specification joins a document. |

## Error conditions

<a id="req-0909"></a>

**REQ-0909.** Submission metadata declared for a column outside
`output.columns`: fail validation and report the column.

<a id="req-0910"></a>

**REQ-0910.** A `metadata` key naming a field this contract governs at that level:
fail validation with `reserved_metadata_key`, reporting the key.

<a id="req-0911"></a>

**REQ-0911.** A `length` that is not a positive integer, or a
`significant_digits` that is negative: fail validation.

<a id="req-0912"></a>

**REQ-0912.** A `length` absent where [REQ-0872](metadata.md#req-0872) requires it, or declared where
that requirement prohibits it; a `significant_digits` absent or declared
against [REQ-0873](metadata.md#req-0873): fail validation with `submission_length_not_applicable` or
`submission_length_missing`, reporting the column.

<a id="req-0913"></a>

**REQ-0913.** A `length` and a `max_length` verification declaring different
bounds on the same column: fail validation with `declared_length_conflict`,
reporting the column and both bounds. A `str` column with neither declaration,
where [REQ-0872](metadata.md#req-0872) requires a length, fails as [REQ-0872](metadata.md#req-0872) states.

<a id="req-0914"></a>

**REQ-0914.** A `data_type` outside the set the column's declared type admits:
fail validation with `submission_data_type_not_admitted`, reporting the column,
the declared type, and the submission type.

<a id="req-0915"></a>

**REQ-0915.** A `core` declared for the `adam` family, absent for `sdtm` or
`send`, a `mandatory: false` on a `Req` column, or a `mandatory` absent for
`adam`: fail with `core_mandatory_conflict`, reporting the column.

<a id="req-0916"></a>

**REQ-0916.** A `mandatory: true` column with neither a `not_missing`
verification nor membership in `keys`: fail with `mandatory_not_enforced`.

<a id="req-0917"></a>

**REQ-0917.** A `role` declared for the `adam` family, or a `domain` declared
for it: fail validation and report the field.

<a id="req-0918"></a>

**REQ-0918.** An `origin` absent from a column carrying submission metadata:
fail with `origin_missing`.

<a id="req-0919"></a>

**REQ-0919.** An origin type and source pair the family does not admit, a
`source` present where [REQ-0889](metadata.md#req-0889) through [REQ-0891](metadata.md#req-0891) require absence, or one absent
where those rules require presence: fail with `origin_source_not_admitted`.

<a id="req-0920"></a>

**REQ-0920.** An `origin.description` absent where [REQ-0892](metadata.md#req-0892) requires it: fail
with `origin_description_missing`.

<a id="req-0921"></a>

**REQ-0921.** A `Collected` origin from `Investigator` or `Subject` with no
document reference: fail with `origin_document_missing`.

<a id="req-0922"></a>

**REQ-0922.** A declared origin the derivation graph refutes under [REQ-0897](metadata.md#req-0897)
through [REQ-0900](metadata.md#req-0900): fail with `origin_contradicts_derivation`, reporting the
column, the declared type, and the derivation that refutes it.

<a id="req-0923"></a>

**REQ-0923.** A `Derived` origin with no `method`: fail with `method_missing`.

<a id="req-0924"></a>

**REQ-0924.** A dataset bound to a standard outside [REQ-0859](metadata.md#req-0859)'s table, or to a
standard of type `CT`: fail with `unknown_standard_family`, reporting the
dataset and the standard. [REQ-0965](define-xml.md#req-0965) owns the binding that fails.

<a id="req-0925"></a>

**REQ-0925.** A `reference_data: true` dataset declaring `repeating: true`:
fail validation and report the dataset.

## Conformance examples

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Govern dataset and column metadata, origin, methods, and document references. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
