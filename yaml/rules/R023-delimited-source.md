---
id: R023
title: Source Profile Selection and Delimited Source Profile
status: normative
applies_to: [dataset_source, dataset_class.path]

---

# Source profile selection and delimited source profile

## Intent

Select the profile a source path names. For a delimited source, fix its bytes,
records, fields, and the spellings two runtimes must read alike or reject.

## Boundaries

This rule owns source-profile selection and the syntax of the `csv` profile.
The profile ends at the field with a header and a sequence of records. Each
delivered field carries its text and whether the field was quoted. R014
owns what those fields mean, which of them is missing, and what type each
one takes. R027 owns the `parquet` source profile. Nothing here decides a
value.

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

**R023-1.** `dataset_class.path` names the file a specification reads, and its
extension selects the profile that reads it. The mapping is closed, so an
extension outside it names no profile and fails validation rather than
falling back to one. The extension is matched without regard to case, because
a study that stores `DM.CSV` names the same container as one that stores
`dm.csv`.

| Extension | Profile | Container | Source-profile owner |
|---|---|---|---|
| `.csv` | `csv` | delimited text | this rule |
| `.parquet` | `parquet` | Apache Parquet | R027 |

**R023-2.** A source profile follows the artifact profile: a file R020 writes
and this rule reads has one profile name in both directions. A second field
could disagree with the source path, misname the contents, and leave the
reader unable to check the claim.

**R023-3.** Sniffing is not permitted. A reader that inspected a file's
contents to choose a delimiter or a quote character could misread a
conforming source without failing. A reader that accepted an unknown
extension under a default could read a container this profile does not
describe at all.

## CSV spellings are admitted only when they carry the same records

**R023-4.** For the `csv` profile, a writer controls its bytes and emits
one spelling, while a reader receives a study file as stored. This rule
admits a second spelling only when both spellings deliver the same records,
and refuses every other difference rather than silently repairing it.

**R023-5.** Admitted: a record terminated by `U+000D U+000A` rather than
`U+000A`, and a final record with no terminator at all.

**R023-6.** Refused: a byte-order mark, a `U+000D` anywhere else, and every
reader option in *Nothing here is configuration*. Each refusal prevents a
change to the records or values the file delivers. Admitting any change would
make two conforming runtimes disagree about the same bytes.

## Encoding

**R023-7.** A delimited source is UTF-8. Ill-formed encoded bytes fail under
R019 rather than being replaced, skipped, or decoded under a machine default.

**R023-8.** A byte-order mark is rejected rather than skipped. Readers that
skip the mark and readers that keep it disagree about the first field's
name, so a marked file has a header whose first name depends on the reader.
Rejecting the mark names the defect for the producer to fix. Skipping the
mark would accept a file whose header this design cannot state.

## Records and fields

**R023-9.** A **record terminator** is `U+000A`, optionally preceded by
`U+000D`. The final record may omit its terminator. A file's records are the
same under either terminator, which is why both are admitted.

**R023-10.** `,` separates the fields of a record. It is the only delimiter.

**R023-11.** A field is either **bare** or **quoted**, and the choice is a
property of the stored field rather than of its value:

- **R023-12.** A **bare** field runs from the delimiter or terminator before
  it to the one after it. It carries no `U+0022`, and no `U+000D`.
- **R023-13.** A **quoted** field opens and closes with `U+0022`. Inside it,
  `U+0022 U+0022` is one literal `U+0022`, and `,` and `U+000A` are ordinary
  characters of the value. A closing quote is followed only by a delimiter or
  a terminator.

**R023-14.** `U+000D` occurs only as the first character of a record
terminator. Inside a quoted field the character fails rather than joining
the value. Without this refusal, a file with `U+000D U+000A` terminators
would deliver a different value than the same file with `U+000A` terminators.
Admitting both terminators is meant to remove exactly that
disagreement.

**R023-15.** Nothing is trimmed. A space beside a delimiter is a character of
the field, and a reader that removes it changes a collected value.

## The header

**R023-16.** The first record is the **header**, and it names the source's
fields in order. Each name is non-empty, and no two names are the same
under R019's equality. A quoted name carries the text inside its quotes, so a
quoted empty name is an empty name and fails.

**R023-17.** Every later record carries exactly as many fields as the header
names. A record with more or fewer fails. Padding a short record and
discarding a long record's surplus are not implementation options. Both
accept a file whose shape the study did not intend.

**R023-18.** A file with no bytes has no header and fails. A header-only
file holds no records, which is valid: an empty dataset still has its fields.

## Nothing here is configuration

**R023-19.** An implementation must not expose, and must not silently apply, a
reader option that changes what this rule fixes. In particular: no comment
prefix or skipped preamble, no alternate delimiter or quote character, no
whitespace trimming, no header synthesis or renaming, and no missing-value
sentinel.

**R023-20.** A missing-value sentinel is easy to mistake for an oversight.
R014 fixes what a stored field means, including that no text spells absence.
A reader option that spells absence decides the meaning of absence before
any rule in this design sees the value.

## Quoting is transport, not meaning

**R023-21.** Every field reaches R014 as its text or as missing, and an
implementation must preserve the text and the missing state. A field with
no characters is missing, bare or quoted. Quoting is a transport detail the
reader does not report. R014 gives an empty field one meaning and never sees
an empty string. Common dataframe readers discard text-versus-missing
distinctions by default; conformance is a property of what the reader
delivers, not of which library produced it.

## Rationale

A second spelling is admitted exactly where both spellings deliver the same
records; every other refused spelling is a file the sponsor repairs before
a run. A reader that skipped a byte-order mark and one that kept it would
disagree about the first field's name, and trimming spaces would change a
collected value. Fixing quoting, width, and names here lets R014 decide
what fields mean on text both runtimes deliver identically.

## Errors

**R023-22.** A failure names the input dataset, the path exactly as the
specification wrote it, and the record and field where it was decided.
Records and fields are counted from one, and the header is record one. A
message carries no host path, for the reason R021 gives.

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
| `source_text_after_quote` | text after a closing quote |
| `source_carriage_return` | `U+000D` that does not begin a record terminator |

`source_text_after_quote` excludes a delimiter or a terminator after the quote.

**R023-23.** `source_profile_unknown` is decided from the written path before
any byte is read and reports under the `validation` phase. Every other
condition is decided while the snapshot is read and reports under the
`ingest` phase.

- **R023-24.** Ill-formed encoded bytes: fail with R019's `invalid_text`,
  reporting the record and field the reader had reached.
- **R023-25.** Repairing a rejected file in the reader -- skipping a mark,
  trimming a field, padding a record, renaming a duplicate name, or
  normalizing a terminator inside a value: none is an implementation option.
