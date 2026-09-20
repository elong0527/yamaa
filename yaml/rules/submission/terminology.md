---
id: submission/terminology
title: Controlled terminology
status: normative
---

# Controlled terminology

## Purpose

Declare codelists once and validate their bindings and allowed values.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Verification](../execution/verification.md).
- [Define-XML](define-xml.md).
- [Text values](../values/text.md).

## Requirements

### One object, many bindings

<a id="req-0926"></a>

**REQ-0926.** A codelist is declared once in the study document and carries an
`id`. A column names that `id` through `submission.codelist`. Columns share
one codelist statement, so a study upgrades a published version in one place.

<a id="req-0927"></a>

**REQ-0927.** `name` is the codelist's human-readable name and is unique across
the document's codelists. `id` and `name` are separate. `id` is what a
specification writes, and `name` is what a reader sees. Forcing `id` and
`name` to match would make renaming a codelist a change to every
specification that binds the codelist.

<a id="req-0928"></a>

**REQ-0928.** `standard` names a declared standard of type `CT` and states which
published terminology this codelist is drawn from. A codelist that omits
`standard` is sponsor-defined rather than unknown. Exactly one of published
or sponsor-defined holds for any codelist.

<a id="req-0929"></a>

**REQ-0929.** `data_type` is `text`, `integer`, or `float` and defaults to
`text`. Every coded value must be of the declared `data_type`.

<a id="req-0930"></a>

**REQ-0930.** `alias` carries the published concept identifier of the codelist
itself, and an item's `alias` carries the published identifier of that value.
Both aliases are optional here, and [Define-XML](define-xml.md) states when a document requires an
alias.

### Values

<a id="req-0931"></a>

**REQ-0931.** A codelist declares either `items` or `external`, and never both
and never neither. A codelist that lists its values and a codelist that defers
to a dictionary are two different objects, and a codelist that did neither
would constrain nothing.

<a id="req-0932"></a>

**REQ-0932.** Each item declares `value`, the coded value itself. `decode` is
the text the value stands for, and is optional: a list whose values are
already the words a reader needs has no decode to add. Declaring `decode` for
some items and not others within one codelist is an error, because lists with
and without `decode` are different kinds of list rather than a list with gaps.

<a id="req-0933"></a>

**REQ-0933.** `rank` states an item's ordering significance relative to the
other items. Declaring `rank` for some items and not others within one
codelist is an error, for the reason [REQ-0932](terminology.md#req-0932) gives.

<a id="req-0934"></a>

**REQ-0934.** `extended` marks an item the sponsor added to a published list.
`extended` defaults to false and may be true only on an `extensible` codelist
that declares `standard`. Adding a value to a list that admits no additions
is a contradiction rather than an extension.

<a id="req-0935"></a>

**REQ-0935.** Coded values are unique within a codelist under [Text values](../values/text.md) equality for
`text` and under numeric equality otherwise. Items keep their declared order;
[Define-XML](define-xml.md) carries the declared order into the generated document.

<a id="req-0936"></a>

**REQ-0936.** An `external` codelist names the `dictionary` and its `version`,
with an optional `href`. Large or volatile dictionaries are referenced; the
version is required because a reference without a version identifies nothing
to check.

### What a binding enforces

<a id="req-0937"></a>

**REQ-0937.** Binding a non-extensible codelist that declares `items` requires
every non-missing value of the column to equal one declared `value`. Missing
values pass, exactly as [Verification](../execution/verification.md)'s `allowed_values` treats missing values; a
column that also prohibits absence declares `not_missing`.

<a id="req-0938"></a>

**REQ-0938.** Binding an extensible codelist enforces nothing. The list states
what is expected and admits a value outside the list, so a check would reject
what the declaration permits.

<a id="req-0939"></a>

**REQ-0939.** Binding an `external` codelist enforces nothing. The admitted
values live in the dictionary and this language does not read the dictionary.

<a id="req-0940"></a>

**REQ-0940.** The enforcement in [REQ-0937](terminology.md#req-0937) runs where [Verification](../execution/verification.md) runs a column
verification, over the completed column, and fails the same way. The
enforcement is the same check `allowed_values` performs, arriving from the
terminology rather than from a second list beside it.

<a id="req-0941"></a>

**REQ-0941.** A bound column's declared type must admit the codelist's
`data_type`: a `text` codelist binds to a `str` column, an `integer` codelist
to an `int` column, and a `float` codelist to a `float` column. A coded value
a column could never hold is a defect in the specification rather than a
constraint that never fires.

### Agreement with allowed_values

<a id="req-0942"></a>

**REQ-0942.** A column may declare both a `codelist` binding and an
`allowed_values` verification. When the bound codelist declares `items`, the
two value sets must be equal: same values, no more and no fewer, compared
under the equality [REQ-0935](terminology.md#req-0935) uses. A difference is rejected.

<a id="req-0943"></a>

**REQ-0943.** Equality is required rather than containment in either
direction. A specification listing fewer values than its terminology asserts a
narrowing that the document does not report, and a specification listing
more asserts values the terminology does not admit. Either way the document
and the run would state different things about the same column, which is the
failure this requirement exists to prevent.

<a id="req-0944"></a>

**REQ-0944.** Binding an extensible or `external` codelist beside an
`allowed_values` verification is accepted, and the verification stands alone.
The terminology admits values outside its list, so there is no set for the
verification to disagree with; narrowing an extensible list for one study's
data is what the standard allows.

<a id="req-0945"></a>

**REQ-0945.** Declaring `allowed_values` without a `codelist` binding remains
an ordinary [Verification](../execution/verification.md) verification. Not every constrained column carries published
terminology.

### Every declared codelist is used

<a id="req-0946"></a>

**REQ-0946.** Every codelist the study document declares must be named by at
least one binding among the datasets the study document represents. An
unreferenced codelist is rejected rather than emitted, because the codelist
would put terminology into a submission that no column carries. The
usual cause is a binding that misspells its identifier.

### Study-inventory vocabulary

<a id="req-1156"></a>

**REQ-1156.** A column carrying submission metadata may declare
`inventory_vocabulary`. Every non-missing value of such a column must equal
the name of a dataset the study document declares. The admitted values are
the study's dataset inventory, which no static list can enumerate; that is
what distinguishes this vocabulary from a codelist, whose values
[REQ-0931](terminology.md#req-0931) requires be listed or deferred to a dictionary. Missing
values pass, exactly as [REQ-0937](terminology.md#req-0937) treats missing values; a column
that also prohibits absence declares `not_missing`. Enforcement runs where
[Verification](../execution/verification.md) runs a column verification, over the
completed column, and fails the same way. The ADaM `SRCDOM` column, whose
values name the source SDTM domain or ADaM dataset, binds this vocabulary.

### Interface behavior

<a id="req-1076"></a>

**REQ-1076.** The `define_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_class.codelists` | Controlled terminology shared by bindings; [Controlled terminology](terminology.md) defines it. |

<a id="req-1077"></a>

**REQ-1077.** The `codelist_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `codelist_class.id` | Name column bindings refer to this codelist by. |
| `codelist_class.name` | Name of the codelist, unique within the document. |
| `codelist_class.standard` | Controlled-terminology standard this codelist is drawn from; omission declares it sponsor-defined. |
| `codelist_class.data_type` | Type of the coded values. |
| `codelist_class.extensible` | Whether a value outside the list is admitted; [Controlled terminology](terminology.md) states what each answer enforces. |
| `codelist_class.alias` | Published concept identifier of the codelist itself. |
| `codelist_class.format_name` | Transport format name carried for a reader. |
| `codelist_class.items` | Coded values this codelist admits, in the order the document presents them. |
| `codelist_class.external` | Dictionary this codelist defers to instead of listing values. |

<a id="req-1078"></a>

**REQ-1078.** The `codelist_item_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `codelist_item_class.value` | The coded value itself. |
| `codelist_item_class.decode` | Text the coded value stands for. |
| `codelist_item_class.rank` | Ordering significance of this value relative to the others. |
| `codelist_item_class.alias` | Published concept identifier of this value. |
| `codelist_item_class.extended` | Whether the sponsor added this value to a published list. |

<a id="req-1079"></a>

**REQ-1079.** The `external_codelist_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `external_codelist_class.dictionary` | Name of the external dictionary. |
| `external_codelist_class.version` | Version of that dictionary. |
| `external_codelist_class.href` | Location the dictionary is published at. |

## Error conditions

<a id="req-0947"></a>

**REQ-0947.** A codelist declaring both `items` and `external`, or neither:
fail validation with `codelist_shape_invalid`, reporting the codelist.

<a id="req-0948"></a>

**REQ-0948.** A duplicate codelist `id` or `name`: fail validation.

<a id="req-0949"></a>

**REQ-0949.** A duplicate coded value within one codelist: fail validation with
`codelist_duplicate_value`.

<a id="req-0950"></a>

**REQ-0950.** A coded value that is not of the codelist's `data_type`: fail
validation.

<a id="req-0951"></a>

**REQ-0951.** `decode` or `rank` declared on some items of a codelist and not
others: fail validation with `codelist_partial_item_field`.

<a id="req-0952"></a>

**REQ-0952.** An `extended: true` item on a sponsor-defined or non-extensible
codelist: fail validation with `codelist_extension_not_admitted`.

<a id="req-0953"></a>

**REQ-0953.** A `codelist` binding naming no declared codelist: fail validation
with `unknown_codelist`, reporting the column and the identifier.

<a id="req-0954"></a>

**REQ-0954.** A binding whose codelist `data_type` the column's declared type
does not admit: fail validation with `codelist_type_mismatch`.

<a id="req-0955"></a>

**REQ-0955.** A `codelist` binding and an `allowed_values` verification whose
value sets differ, where [REQ-0942](terminology.md#req-0942) requires equality: fail validation with
`codelist_values_conflict`, reporting the column and both sets.

<a id="req-0956"></a>

**REQ-0956.** A declared codelist no binding names: fail validation with
`unreferenced_codelist`, reporting the codelist.

<a id="req-0957"></a>

**REQ-0957.** A non-missing value outside a bound non-extensible codelist: fail
where [Verification](../execution/verification.md) fails a column verification, reporting the column, the row's key,
and the value.

<a id="req-0958"></a>

**REQ-0958.** A `standard` naming a declared standard whose type is not `CT`:
fail validation.

## Conformance examples

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Declare codelists once and validate their bindings and allowed values. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
