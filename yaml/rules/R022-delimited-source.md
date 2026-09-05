---
id: R022
title: Delimited Source Profile
status: normative
applies_to: [dataset_source, dataset_class.path]
depends_on: [R014, R019, R021]
---

# Delimited source profile

## Intent

Fix the bytes a delimited source carries: which file is one, how its records
and fields are spelled, which spellings two runtimes must read the same way,
and which they must reject instead of interpreting.

## Boundaries

This rule owns the syntax of a delimited source. It ends at the field: it
delivers a header and a sequence of records whose fields carry their text and
whether they were quoted, and R014 owns what those fields mean, which of them
is missing, and what type each one takes. Nothing here decides a value.

R020 owns the other direction, and its `csv` profile is the writing
counterpart of the form this rule reads. The two agree on the bytes, and
neither restates the other. They differ in one respect only, stated below: a
writer emits one spelling, while a reader receives files it did not write and
admits every spelling that cannot carry different data.

R021 owns which file `path` may reach and the immutable byte snapshot this
rule parses, so a file that reaches this rule has already been accepted there.
R019 owns valid text and the failure of ill-formed encoded bytes. R002 owns
which datasets a specification declares and how a name binds to one. R011 and
R016 own the value a delivered field parses into.

## The path's extension selects the profile

`dataset_class.path` names the file a specification reads, and its extension
selects the profile that reads it. The mapping is closed, so an extension
outside it names no profile and fails validation rather than falling back to
one. The extension is matched without regard to case, because a study that
stores `DM.CSV` names the same container as one that stores `dm.csv`.

| Extension | Profile | Container |
|---|---|---|
| `.csv` | `csv` | delimited text |

A source is selected the same way an artifact is, so a file this design writes
under R020 and then reads back is described by one profile name in both
directions, and a reviewer reads that name off the path in either place. A
second field beside the path could disagree with it, and a source whose
declaration says `csv` while its name says otherwise is a file whose name lies
about its contents in the one direction where the reader cannot check.

Sniffing is not permitted. A reader that inspected a file's contents to choose
a delimiter or a quote character could read a conforming source wrongly
without failing, and a reader that accepted an unknown extension under a
default could read a container this profile does not describe at all.

## Two spellings are admitted only when they carry the same records

A writer controls its own bytes and emits one spelling of them. A reader
receives a study's file as the study stores it, and every spelling it refuses
is a file a sponsor must repair before a run can proceed. This rule therefore
admits a second spelling exactly where the two cannot deliver different
records, and refuses every other difference rather than repairing it silently.

Admitted: a record terminated by `U+000D U+000A` rather than `U+000A`, and a
final record with no terminator at all. Every reader agrees on the records
these files hold.

Refused: a byte-order mark, a `U+000D` anywhere else, and every reader option
in *Nothing here is configuration*. Each of those changes which records or
which values a file delivers, so admitting one would make two conforming
runtimes disagree about the same bytes.

## Encoding

A delimited source is UTF-8. Ill-formed encoded bytes fail under R019 rather
than being replaced, skipped, or decoded under a machine default.

A byte-order mark is rejected rather than skipped. A reader that skips one and
a reader that keeps one disagree about the first field's name, so a file that
carries one has a header whose first name depends on who reads it. Rejecting
it names the defect where a producer can fix it, and skipping it would accept
a file whose header this design cannot state.

## Records and fields

A **record terminator** is `U+000A`, optionally preceded by `U+000D`. The
final record may omit its terminator. A file's records are the same under
either terminator, which is why both are admitted.

`,` separates the fields of a record. It is the only delimiter.

A field is either **bare** or **quoted**, and the choice is a property of the
stored field rather than of its value:

- A **bare** field runs from the delimiter or terminator before it to the one
  after it. It carries no `U+0022`, and no `U+000D`.
- A **quoted** field opens and closes with `U+0022`. Inside it, `U+0022
  U+0022` is one literal `U+0022`, and `,` and `U+000A` are ordinary
  characters of the value. A closing quote is followed only by a delimiter or
  a terminator.

`U+000D` occurs only as the first character of a record terminator. Inside a
quoted field it fails rather than joining the value, because a file whose
terminators are `U+000D U+000A` would otherwise deliver a different value than
the same file written with `U+000A`, and admitting both terminators is meant
to remove exactly that disagreement.

Nothing is trimmed. A space beside a delimiter is a character of the field,
and a reader that removes it changes a collected value.

## The header

The first record is the **header**, and it names the source's fields in
order. Each name is non-empty, and no two are the same name under R019's
equality. A quoted name carries the text inside its quotes, so a quoted empty
name is an empty name and fails.

Every later record carries exactly as many fields as the header names. A
record with more or fewer fails; neither padding a short record nor discarding
a long one's surplus is an implementation option, because both accept a file
whose shape the study did not intend.

A file with no bytes has no header and fails. A file whose only record is the
header is a source with no records, which is not a failure: a dataset a study
collected nothing into still has its fields.

## Nothing here is configuration

An implementation must not expose, and must not silently apply, a reader
option that changes what this rule fixes. In particular: no comment prefix or
skipped preamble, no alternate delimiter or quote character, no whitespace
trimming, no header synthesis or renaming, and no missing-value sentinel.

The sentinel case is the one whose absence is easiest to mistake for an
oversight. R014 fixes what a stored field means, including that no text spells
absence, and a reader option that spelled it here would decide that question
before any rule in this design could see the value.

## Quoting is delivered, not erased

Every field reaches R014 as its text together with whether it was quoted, and
an implementation must preserve both. A reader that returns text alone cannot
tell a bare empty field from a quoted empty one, and R014 gives those two
different meanings, so such a reader does not implement this profile even
though it reads every other field correctly. Common dataframe readers discard
this distinction by default; conformance is a property of what the reader
delivers, not of which library produced it.

## Errors

A failure names the dataset, the path exactly as the specification wrote it,
and the record and field where it was decided. Records and fields are counted
from one, and the header is record one. A message carries no host path, for
the reason R021 gives.

| Condition | Rejects |
|---|---|
| `source_profile_unknown` | an extension the mapping above does not name |
| `source_byte_order_mark` | a byte-order mark |
| `source_header_absent` | a file with no header record |
| `source_field_name_empty` | an empty header name |
| `source_field_name_duplicate` | one name twice in the header |
| `source_record_width` | a record whose field count is not the header's |
| `source_quote_unterminated` | a quoted field with no closing quote |
| `source_quote_in_bare_field` | `U+0022` inside a bare field |
| `source_text_after_quote` | anything but a delimiter or a terminator after a closing quote |
| `source_carriage_return` | `U+000D` that does not begin a record terminator |

`source_profile_unknown` is decided from the written path before any byte is
read and reports under the `validation` phase. Every other condition is
decided while the snapshot is read and reports under the `ingest` phase.

- Ill-formed encoded bytes: fail with R019's `invalid_text`, reporting the
  record and field the reader had reached.
- Repairing a rejected file in the reader -- skipping a mark, trimming a
  field, padding a record, renaming a duplicate name, or normalizing a
  terminator inside a value: none is an implementation option.
