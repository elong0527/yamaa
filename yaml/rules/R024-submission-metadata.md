---
id: R024
title: Submission Metadata
status: normative
applies_to: [root.submission, column.submission, submission_dataset_class,
  submission_column_class, submission_origin_class, submission_method_class,
  submission_comment_class, document_reference_class]
---

# Submission metadata

## Intent

Give a dataset and every column it declares one closed, typed place to carry
the metadata a submission document represents: what the dataset is, what each
column means, how long its values may be, where its values came from, and what
algorithm produced them. Close the vocabularies, state which combinations each
standard admits, and fix what the derivation graph may refute so a declared
provenance is checked rather than believed.

## Boundaries

This rule owns the declared vocabulary and the consistency of one
specification's declarations. It does not own the document those declarations
are composed into: R026 owns the study document, the identifiers, the element
mapping, the order, and the bytes. It does not own controlled terminology:
R025 owns the codelist object a `codelist` binding names and what that binding
enforces.

Every identifier this rule admits -- `codelist` and a document reference --
names an object the study document declares. A specification therefore carries
the names without resolving them, and R026 resolves them and applies every
requirement below that depends on a standard's family. The standard itself is
not named here at all: R026-7 binds it to the dataset.

R005 owns which columns the artifact has and their order, R009 owns the
assertions a value is checked against, R011 owns the declared column type, and
R020 owns the artifact's bytes. Nothing here changes any of them: submission
metadata describes an artifact those rules already decide, and no field below
can change a value, a row, or a column.

## Where it is declared

**R024-1.** `root.submission` carries the dataset's metadata and
`column.submission` carries one column's. Both are optional in the schema,
because a specification that no submission document represents needs neither.
R026 requires them from a specification that joins a document.

**R024-2.** Submission metadata is declared only for a column `output.columns`
carries. An internal column is not in the artifact, so it is not in the
document that describes the artifact, and declaring metadata for one is an
error rather than an ignored declaration.

**R024-3.** The free-form `metadata` map keeps its purpose. It remains
uninterpreted annotation, and no field below is read from it. A study that
carried governed facts in that map moves them here; the two are not merged and
the map is never a fallback.

**R024-4.** The map must not carry a key this rule governs. Root `metadata`
rejects every field name of the dataset object above, and a column's
`metadata` rejects every field name of the column object below. This is what
makes R024-3 checkable rather than asserted: without it a study could write
`metadata` with an `origin` key beside a governed `origin`, leaving two
provenance claims in one specification and nothing to say which one the
document reports.

## Standard families

**R024-5.** Every dataset follows one foundational standard, and the study
document binds it: R026-7 resolves each dataset to a declared standard of type
`IG`. A specification does not name it. Which release of an implementation
guide a study submits against is a study decision that changes between
submissions, so binding it in the specification would version every dataset
template against one study. This rule reads the bound standard's published
name as one of three closed **families**:

| Family | Standard names |
|---|---|
| `sdtm` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD` |
| `send` | `SENDIG`, `SENDIG-AR`, `SENDIG-DART`, `SENDIG-GENETOX` |
| `adam` | `ADaMIG`, `ADaMIG-MD` |

**R024-6.** The table is closed. A dataset bound to an `IG` standard outside it
fails: the origin pairs, the core mapping, and the domain and purpose
decisions below are all family-specific, and this design closes none of them
for a standard it does not name. `BIMO` is the standard this exclusion is
visible for today, and admitting it is a change to this table rather than an
implementation's judgment.

**R024-7.** A dataset must not be bound to a standard of type `CT`.
Controlled terminology qualifies a codelist and not a dataset; R025 owns that
binding.

## Dataset metadata

**R024-8.** `label` states what the dataset is, and is required. It is the
dataset's own description and is distinct from `domain`: `DM` is the domain
and `Demographics` is the label.

**R024-9.** `class` names the general observation class and is required, in
the exact spelling and case the vocabulary publishes. `subclass` is declared
only when the dataset's implementation guide defines one for it.

**R024-10.** `structure` states in prose the level of detail one record
represents, such as `One record per subject`. It is required, because a reader
cannot infer a dataset's grain from its keys alone: the same key list serves
several structures.

**R024-11.** `repeating` is required and states whether the dataset may carry
more than one record per subject or pool. `reference_data` defaults to false
and states whether the dataset carries reference data rather than subject
data. A dataset declaring `reference_data: true` must declare
`repeating: false`.

**R024-12.** `domain` names the domain the dataset belongs to. It defaults to
the specification's `domain`, so an ordinary domain states it once. A split or
supplemental dataset declares it, because its own name is not its parent
domain.

**R024-13.** `domain` is declared only for a dataset whose family is `sdtm` or
`send`. The `adam` family has no domain, so declaring one there is an error
rather than an unused value.

## Column metadata

### Submission data type

**R024-14.** `data_type` states the type the submission represents the column
as. It defaults from the declared column type, and each declared type admits a
closed set:

| Declared type | Default | Also admits |
|---|---|---|
| `str` | `text` | `date`, `datetime`, `time`, `partialDate`, `partialTime`, `partialDatetime`, `incompleteDate`, `incompleteTime`, `incompleteDatetime`, `durationDatetime`, `intervalDatetime`, `URI` |
| `int` | `integer` | none |
| `float` | `float` | none |
| `date` | `date` | none |
| `datetime` | `datetime` | none |

**R024-15.** A `str` column admits the temporal submission types because a
submission carries a partial or incomplete date as text, which R016 does not
admit as a `date` or a `datetime`. The two temporal column types admit only
their own submission type, because R016 defines each as a complete value and
the submission types of the same name mean the same thing. A number's
submission type is fixed, because R011 already decided whether the column
holds an integer or a binary64.

**R024-16.** `data_type` never changes a value. It states how a value R011
already typed is represented in the document, and no derivation, verification,
key, order term, or artifact byte can observe it.

### Length and significant digits

**R024-17.** `length` is the maximum expected value length, as the submission
standard means it: a property of the column's declaration rather than of the
data a particular run produced. It is a positive integer.

**R024-18.** `length` is required when `data_type` resolves to `text`,
`integer`, or `float` unless R024-21 derives it, and must not be declared
otherwise. The remaining
submission types carry values of a fixed written form, so a length for one
would be a second statement of that form.

**R024-19.** `significant_digits` is required when `data_type` resolves to
`float` and must not be declared otherwise. It is a non-negative integer
stating how many digits follow the decimal point.

**R024-20.** A `length` on a `str` column is enforced. The column's completed
values must contain at most `length` R019 scalar values, which is exactly what
R009's `max_length` requires, so the declared length is the constraint rather
than a note beside one.

**R024-21.** On a `str` column carrying a `max_length` verification, `length`
is derived from that verification's `max` and need not be declared. The two
state one bound, and stating it twice only creates a way for them to disagree.
Declaring both is accepted when they are equal and rejected when they differ,
and the bound is checked once either way.

**R024-22.** A `length` declared on any other column type is not enforced.
R009 states why: the text a number or a temporal value renders as is a
property of R011's and R020's rendering rather than of the value, so a length
over it would assert something this language does not decide. The declaration
is carried into the document unchecked, and that limit is stated rather than
hidden. Enforcing a rendered width is a change to R009's `max_length` and to
this requirement together.

### Display format and role

**R024-23.** `display_format` is presentation text carried for a reader. It
never changes a computed value, a rendered artifact value, or R020's
`output.decimals`. A study that wants an artifact's digits to change declares
`output.decimals`, which R020 owns; the two are independent and this rule does
not reconcile them, because one is a display a document reports and the other
is a display an artifact carries.

**R024-24.** `role` states how the column is used within its dataset, in the
vocabulary its standard publishes. This language does not close that
vocabulary, because each implementation guide publishes and versions its own.

**R024-25.** `role` must not be declared for a dataset whose family is `adam`.
That family defines no role vocabulary, so a role there would name nothing.

### Core and mandatory

**R024-26.** `core` is the standard's core designation: `Req` for required,
`Exp` for expected, or `Perm` for permissible. It is declared for a dataset
whose family is `sdtm` or `send`, and must not be declared for `adam`.

**R024-27.** `mandatory` states whether the completed column must carry a
value. It is not a second spelling of `core`, and the mapping between them is
standard-specific:

- **R024-28.** For `sdtm` and `send`, `core` is required and `mandatory`
  defaults from it: `Req` gives true, and `Exp` and `Perm` give false.
- **R024-29.** For `sdtm` and `send`, a declared `mandatory: true` on an `Exp`
  or `Perm` column is an accepted sponsor restriction. A declared
  `mandatory: false` on a `Req` column is rejected: the standard requires the
  variable, and a specification cannot relax its own standard by declaring
  around it.
- **R024-30.** For `adam`, `mandatory` is required and nothing derives it.
  That family publishes no general one-to-one mapping from its core
  designation, so inferring one would be this design inventing a rule its
  standard does not state.

**R024-31.** A column declaring `mandatory: true` must also carry a
`not_missing` verification, or be listed in `keys`, whose non-missing
requirement R005 already states. A document that asserts a value is always
present, over an artifact nothing checks, asserts something no run proves.

### Codelist

**R024-32.** `codelist` names a codelist the study document declares. R025
owns that object, the values a binding enforces, and its agreement with an
`allowed_values` verification.

## Origin

**R024-33.** `origin` is required for every column carrying submission
metadata. A submission document states where every value came from, and a
column with no declared origin has no place in one.

**R024-34.** `origin.type` is one of `Assigned`, `Collected`, `Derived`,
`Not Available`, `Other`, `Predecessor`, or `Protocol`. `origin.source` is one
of `Investigator`, `Sponsor`, `Subject`, or `Vendor`. Both vocabularies are
closed and neither is extended by a specification.

**R024-35.** Which pairs are admitted depends on the family. For `sdtm`:

| Type | Admitted sources |
|---|---|
| `Collected` | `Subject`, `Investigator`, `Vendor` |
| `Derived` | `Vendor`, `Sponsor` |
| `Assigned` | `Vendor`, `Sponsor` |
| `Protocol` | `Sponsor` |
| `Other` | any |
| `Predecessor` | none; `source` must be absent |
| `Not Available` | not admitted |

**R024-36.** For `adam`, the admitted types are `Derived`, `Assigned`,
`Predecessor`, and `Other`, and `source` is derived rather than declared:
`Sponsor` for `Derived`, `Assigned`, and `Other`, and absent for
`Predecessor`. The family fixes it completely, so a declared `source` could
only repeat the derivation or contradict it, and declaring one is rejected.
The standard
describes `Collected` and `Protocol` as generally unused there rather than
forbidden, and this design closes them out: an analysis dataset value that
was neither computed, copied, nor assigned is the case `Other` exists for, and
it carries the description that says what happened.

**R024-37.** For `send`, every type is admitted and `source` must be absent.
That family identifies responsibility through its own trial-summary and
findings variables rather than through this attribute.

**R024-38.** `origin.description` is required when `type` is `Predecessor`,
and names the dataset and column the value was copied from in the form
`<dataset>.<column>`. It is also required when `type` is `Other` or
`Not Available`, because neither states anything on its own. It is optional
elsewhere.

**R024-39.** A value said to be collected from a person is traceable to the
form it was collected on. A column whose `origin.type` is `Collected` with a
`source` of `Investigator` or `Subject` therefore carries a reference to the
study's annotated case report form, and R026 rejects a reference to any other
document there.

**R024-40.** That reference is derived when `documents` is omitted, because
the annotated case report form is the only document it could name. A column
declares `documents` to say where in that form the value was collected;
omitting it leaves the reference without a page.

**R024-41.** A column whose `origin.type` is `Derived` must declare `method`.
The algorithm is what a derived origin claims exists, and a claim with no
algorithm beside it is not traceable.

### What the graph refutes

**R024-42.** The derivation graph R001 builds proves some facts about a
column's value, and a declared origin that contradicts a proven fact is
rejected. The graph never supplies an origin. It cannot distinguish a value an
investigator recorded from one a vendor transmitted or one a protocol fixed,
and a design that guessed between them would put an unverifiable claim into a
submission document. Origin is therefore always declared and sometimes
refuted, never inferred.

**R024-43.** A column whose derivation is anything other than a bare `source`
or a bare `literal` computes its value from other values. Its `origin.type`
must be `Derived`, `Assigned`, or `Other`. `Collected`, `Protocol`,
`Predecessor`, and `Not Available` are refuted: each asserts the value arrived
as it stands, and the specification shows it did not.

**R024-44.** A column derived by a bare `literal` takes the same value in
every row regardless of any input. Its `origin.type` must be `Assigned`,
`Protocol`, or `Other`. `Collected`, `Derived`, `Predecessor`, and
`Not Available` are refuted: nothing was observed, computed, or copied, and a
value the specification states outright is available.

**R024-45.** A column derived by a bare `source` copies one value of one
declared dataset. Its `origin.type` must not be `Derived`: nothing was
computed. Every remaining type stays admissible, because whether that stored
value was collected, assigned, fixed by the protocol, or copied from a
predecessor dataset is a fact about the source and not about this
specification.

**R024-46.** A row-derived column is refuted by the derivation each `rows`
entry declares, applying R024-43 through R024-45 to every entry. Entries that
disagree about which types survive leave the column with the intersection, and
an empty intersection is an error rather than a silently admitted origin.

## Method

**R024-47.** `method` states the algorithm that produces a column's value.
Written as a string it is the algorithm's prose description; written as a
mapping it adds a name, a kind, machine-readable code, and supporting
documents.

**R024-48.** `description` is required, so a reader who cannot evaluate the
code still learns what the column means. `type` defaults to `Computation` and
is `Imputation` when the method replaces a missing value with a substitute.

**R024-49.** `expression` carries code as text under a named `context`. This
language neither parses nor evaluates it, and a generated document carries it
unchanged. Nothing checks that the code computes what the specification's own
derivation computes, and that limit is stated rather than implied: an
expression is documentation a sponsor supplies, not a second definition of the
column.

**R024-50.** No `expression` is generated from a derivation. A derivation has
no canonical written form in this language, and inventing one here would fix a
printing of every expression the registry holds without a rule that owns it.
A future rule may define that printing and generate the expression from it;
until one does, the field is declared or absent.

## Comment and documents

**R024-51.** `comment` carries a sponsor note about a dataset or a column.
Written as a string it is the note's text; written as a mapping it adds the
documents the note refers to. Comments are declared where they are used, and
one comment is not shared between two datasets or two columns. R026 mints each
one an identifier from the place it is declared, so a shared comment would
need an identifier space this design does not open.

**R024-52.** A document reference names a document the study document
declares, and optionally the place within it. Written as a string it is the
identifier; written as a mapping it adds `pages`. A `pages` reference states
whether its `refs` are physical page numbers or named destinations, and
carries them as a space-separated list.

## What a specification alone validates

**R024-53.** Every requirement above that does not depend on a family is
checked when the specification is validated on its own: the vocabularies, the
required and prohibited field combinations, the length binding in R024-20, the
graph refutations in R024-42 through R024-46, and the presence of `method` for
a declared `Derived` origin.

**R024-54.** Every family-dependent requirement is checked when R026 composes
the specification into a document, because only the study document says which
standard the identifier names. A specification that is never composed carries
declarations no family has judged, and that is the correct outcome: it is not
yet part of a submission.

## Rationale

The metadata a submission document carries is not a documentation vocabulary a
project may invent, so every field here is closed against the document it
becomes, and each one has exactly one place to live. Origin is declared rather
than inferred because the dependency graph can prove that a value was computed
but cannot prove who collected it; it is refuted rather than believed because
a graph that proves a value was computed also proves a `Collected` claim
false. Core and mandatory are kept apart because they answer different
questions -- what the standard requires of every study, and what this study
requires of itself -- and because their relationship differs between
standards. Length binds to `max_length` on strings so that a declared bound is
a checked bound, and stays unenforced on numbers for the reason R009 already
gives: a rendered width is a property of rendering. Comments and methods are
declared where they are used because sharing them would need an identifier
space beyond the one R026 derives from position.

## Errors

**R024-55.** Submission metadata declared for a column outside
`output.columns`: fail validation and report the column.
**R024-56.** A `metadata` key naming a field this rule governs at that level:
fail validation with `reserved_metadata_key`, reporting the key.
**R024-57.** A `length` that is not a positive integer, or a
`significant_digits` that is negative: fail validation.
**R024-58.** A `length` absent where R024-18 requires it, or declared where it
prohibits it; a `significant_digits` absent or declared against R024-19: fail
validation with `submission_length_not_applicable` or
`submission_length_missing`, reporting the column.
**R024-59.** A `length` and a `max_length` verification declaring different
bounds on the same column: fail validation with `declared_length_conflict`,
reporting the column and both bounds. A `str` column with neither, where
R024-18 requires a length, fails as that requirement states.
**R024-60.** A `data_type` outside the set its declared column type admits:
fail validation with `submission_data_type_not_admitted`, reporting the column,
the declared type, and the submission type.
**R024-61.** A `core` declared for the `adam` family, absent for `sdtm` or
`send`, a `mandatory: false` on a `Req` column, or a `mandatory` absent for
`adam`: fail with `core_mandatory_conflict`, reporting the column.
**R024-62.** A `mandatory: true` column with neither a `not_missing`
verification nor membership in `keys`: fail with `mandatory_not_enforced`.
**R024-63.** A `role` declared for the `adam` family, or a `domain` declared
for it: fail validation and report the field.
**R024-64.** An `origin` absent from a column carrying submission metadata:
fail with `origin_missing`.
**R024-65.** An origin type and source pair the family does not admit, a
`source` present where R024-35 through R024-37 require its absence, or a
`source` absent where they require it: fail with `origin_source_not_admitted`.
**R024-66.** An `origin.description` absent where R024-38 requires it: fail
with `origin_description_missing`.
**R024-67.** A `Collected` origin from `Investigator` or `Subject` with no
document reference: fail with `origin_document_missing`.
**R024-68.** A declared origin the derivation graph refutes under R024-43
through R024-46: fail with `origin_contradicts_derivation`, reporting the
column, the declared type, and the derivation that refutes it.
**R024-69.** A `Derived` origin with no `method`: fail with `method_missing`.
**R024-70.** A dataset bound to a standard outside R024-5's table, or to one
of type `CT`: fail with `unknown_standard_family`, reporting the dataset and
the standard. R026-7 owns the binding that fails.
**R024-71.** A `reference_data: true` dataset declaring `repeating: true`:
fail validation and report the dataset.
