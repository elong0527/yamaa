---
id: R025
title: Controlled Terminology
status: normative
applies_to: [define.codelists, codelist_class, codelist_item_class,
  external_codelist_class, column.submission.codelist]
---

# Controlled terminology

## Intent

Make a codelist one named, versioned object that several columns share, state
exactly what binding one enforces, and reconcile that binding with the
`allowed_values` verification a specification may also declare, so a study
never states its terminology twice and never states it two ways.

## Boundaries

This rule owns the codelist object, its identity, its values, its
extensibility, its external form, and what a binding enforces. R024 owns the
`codelist` field that names one and every other governed column attribute.
R026 owns the study document the codelists are declared in, the identifiers
the generated document carries, and the order it presents them in. R009 owns
the `allowed_values` verification this rule reconciles a binding with, and
R019 owns the string equality both compare under. R011 owns the declared
column type a coded value must be.

## One object, many bindings

**R025-1.** A codelist is declared once, in the study document, and carries an
`id`. A column names that `id` through `submission.codelist`. Terminology is
therefore stated once however many columns carry it, which is the property
that lets a study upgrade a published version in one place.

**R025-2.** `name` is the codelist's human-readable name and is unique across
the document's codelists. `id` and `name` are separate because `id` is what a
specification writes and `name` is what a reader sees, and constraining one to
the other would make renaming a codelist a change to every specification that
binds it.

**R025-3.** `standard` names a declared standard of type `CT` and states which
published terminology this codelist is drawn from. A codelist that omits
`standard` is sponsor-defined. Exactly one of the two states is true of any
codelist, and the omission is the declaration rather than an unknown.

**R025-4.** `data_type` is `text`, `integer`, or `float` and defaults to
`text`. Every coded value must be of that type.

**R025-5.** `alias` carries the published concept identifier of the codelist
itself, and an item's `alias` carries the published identifier of that value.
Both are optional here and R026 states when a document requires them.

## Values

**R025-6.** A codelist declares either `items` or `external`, and never both
and never neither. A codelist that lists its values and a codelist that defers
to a dictionary are two different objects, and one that did neither would
constrain nothing.

**R025-7.** Each item declares `value`, the coded value itself. `decode` is
the text that value stands for, and is optional: a list whose values are
already the words a reader needs has no decode to add. Declaring `decode` for
some items and not others within one codelist is an error, because the two
forms are different kinds of list rather than a list with gaps.

**R025-8.** `rank` states an item's ordering significance relative to the
others. Declaring it for some items and not others within one codelist is an
error, for the same reason.

**R025-9.** `extended` marks an item the sponsor added to a published list. It
defaults to false, may be true only on a codelist that declares `standard`,
and may be true only on a codelist that is `extensible`. Adding a value to a
list that admits no additions is a contradiction rather than an extension.

**R025-10.** Coded values are unique within a codelist under R019 equality for
`text` and under numeric equality otherwise. Items keep the order the document
declares them in; R026 carries that order into the generated document.

**R025-11.** An `external` codelist names the `dictionary` and its `version`,
and may name the `href` it is published at. A dictionary too large or too
volatile to restate is referenced rather than copied, and the version is
required because a reference without one identifies nothing checkable.

## What a binding enforces

**R025-12.** Binding a non-extensible codelist that declares `items` requires
every non-missing value of the column to equal one declared `value`. Missing
values pass, exactly as R009's `allowed_values` treats them; a column that
also prohibits absence declares `not_missing`.

**R025-13.** Binding an extensible codelist enforces nothing. The list states
what is expected and admits a value outside it, so a check would reject what
the declaration permits.

**R025-14.** Binding an `external` codelist enforces nothing. The admitted
values live in the dictionary and this language does not read it.

**R025-15.** The enforcement in R025-12 runs where R009 runs a column
verification, over the completed column, and fails the same way. It is the
same check `allowed_values` performs, arriving from the terminology rather
than from a second list beside it.

**R025-16.** A bound column's declared type must admit the codelist's
`data_type`: a `text` codelist binds to a `str` column, an `integer` codelist
to an `int` column, and a `float` codelist to a `float` column. A coded value
a column could never hold is a defect in the specification rather than a
constraint that never fires.

## Agreement with allowed_values

**R025-17.** A column may declare both a `codelist` binding and an
`allowed_values` verification. When the bound codelist declares `items`, the
two value sets must be equal: same values, no more and no fewer, compared
under the equality R025-10 uses. A difference is rejected.

**R025-18.** Equality is required rather than containment in either
direction. A specification listing fewer values than its terminology asserts a
narrowing that the document does not report, and one listing more asserts
values the terminology does not admit. Either way the document and the run
would state different things about the same column, which is the failure this
requirement exists to prevent.

**R025-19.** Binding an extensible or `external` codelist beside an
`allowed_values` verification is accepted, and the verification stands alone.
The terminology admits values outside its list, so there is no set for the
verification to disagree with, and a study narrowing an extensible list for
its own data is doing something the standard allows.

**R025-20.** Declaring `allowed_values` without a `codelist` binding is
unchanged and remains an ordinary R009 verification. Not every constrained
column carries published terminology.

## Every declared codelist is used

**R025-21.** Every codelist the study document declares must be named by at
least one binding among the datasets that document represents. An unreferenced
codelist is rejected rather than emitted, because it would put terminology
into a submission that no variable carries, and because the usual cause is a
binding that misspells its identifier.

## Rationale

One object shared by many bindings is what makes a controlled-terminology
version upgradable: a study that restates its terminology per variable has as
many places to change as it has variables, and no way to prove they agree. The
binding enforces only where the terminology is closed, because enforcing an
extensible or external list would reject values the standard admits. Agreement
with `allowed_values` is exact equality because any other relation leaves the
generated document and the executed run making different claims about the same
column, and a submission document whose claims the run does not support is the
specific failure this design exists to prevent. Decode and rank are all-or-
nothing within a codelist because a partially decoded list is two kinds of
list mixed together, and a reader cannot tell an omitted decode from an absent
one.

## Errors

**R025-22.** A codelist declaring both `items` and `external`, or neither:
fail validation with `codelist_shape_invalid`, reporting the codelist.
**R025-23.** A duplicate codelist `id` or `name`: fail validation.
**R025-24.** A duplicate coded value within one codelist: fail validation with
`codelist_duplicate_value`.
**R025-25.** A coded value that is not of the codelist's `data_type`: fail
validation.
**R025-26.** `decode` or `rank` declared on some items of a codelist and not
others: fail validation with `codelist_partial_item_field`.
**R025-27.** An `extended: true` item on a sponsor-defined or non-extensible
codelist: fail validation with `codelist_extension_not_admitted`.
**R025-28.** A `codelist` binding naming no declared codelist: fail validation
with `unknown_codelist`, reporting the column and the identifier.
**R025-29.** A binding whose codelist `data_type` the column's declared type
does not admit: fail validation with `codelist_type_mismatch`.
**R025-30.** A `codelist` binding and an `allowed_values` verification whose
value sets differ, where R025-17 requires equality: fail validation with
`codelist_values_conflict`, reporting the column and both sets.
**R025-31.** A declared codelist no binding names: fail validation with
`unreferenced_codelist`, reporting the codelist.
**R025-32.** A non-missing value outside a bound non-extensible codelist: fail
where R009 fails a column verification, reporting the column, the row's key,
and the value.
**R025-33.** A `standard` naming a declared standard whose type is not `CT`:
fail validation.
