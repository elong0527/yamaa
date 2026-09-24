---
id: storage/csv
title: CSV profile
status: normative
---

# CSV profile

## Purpose

Read admitted CSV spellings and write canonical CSV bytes and display precision.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Specification structure](../specification/structure.md).
- [Source ingestion](ingestion.md).
- [Resource resolution](resources.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).

## Requirements

### Writing: The csv profile

<a id="req-0722"></a>

**REQ-0722.** The artifact is UTF-8 and has no byte-order mark. [Text values](../values/text.md)
  owns the text being encoded.

<a id="req-0723"></a>

**REQ-0723.** `U+000A` terminates every record, including the last, so every
  artifact ends with it. `U+000D` is never written as part of a terminator; it
  appears only inside a quoted field that contains one.

<a id="req-0724"></a>

**REQ-0724.** `U+002C` separates fields. No other delimiter is defined.

<a id="req-0725"></a>

**REQ-0725.** The first record is the header: `output.columns` names, in
  that order, written under the quoting rule below.

<a id="req-0726"></a>

**REQ-0726.** Each following record is one row, in the order [Ordering](../execution/ordering.md) fixes,
  holding one field per header name in the same order.

<a id="req-0727"></a>

**REQ-0727.** An artifact with no rows is the header record and its terminator
  alone. It is not empty. The columns exist whether or not a row
  does.

#### Quoting

<a id="req-0728"></a>

**REQ-0728.** A field is quoted exactly when its text contains `U+0022`,
`U+002C`, `U+000D`, or `U+000A`, or when the field is the empty string. Every
other field is written bare.

<a id="req-0729"></a>

**REQ-0729.** A quoted field is wrapped in `U+0022` and each `U+0022` within its
text is written twice. Nothing else is escaped: a quoted field carries its
newlines, delimiters, and every other scalar exactly.

<a id="req-0730"></a>

**REQ-0730.** The exact condition makes both runtimes quote the same fields. A
writer that quotes a field the condition leaves bare, or leaves bare a field
the condition quotes, does not conform even if ordinary readers accept its
output.

#### Missing and the empty string

<a id="req-0731"></a>

**REQ-0731.** A missing value is written as no characters at all, unquoted. A
collected empty string is written as two quote characters. The two forms stay
apart in the artifact, and no text is ever pressed into service as a sentinel
for absence. Reading does not restore the pair: [REQ-0529](ingestion.md#req-0529) reads an empty field as
missing whether it was bare or quoted, so a collected empty string written here
returns as missing if this artifact is later read as a delimited source. The
asymmetry is deliberate. A source is authored by a producer this language does
not control. A distinction no such producer reliably spells is not one a
reader may invent. An artifact this contract writes has one writer and can
afford the finer form. The `parquet` profile carries the pair in its container
and keeps it in both directions.

    STUDYID,COMMENT,NOTE
    S1,plain text,
    S1,"has, comma",""
    S1,"say ""hi""",x

The third row's `NOTE` is the ordinary string `x`. The second row's is a
collected empty string. The first row's is missing.

#### Value text

<a id="req-0732"></a>

**REQ-0732.** Value text by column type:

| Column type | Text written |
|---|---|
| `str` | its scalar values, under [Text values](../values/text.md) |
| `int` | its decimal digits, with a leading `U+002D` when negative |
| `float` | [Types and conversion](../values/types.md)'s float text, or the fixed-point form below |
| `date` | [Temporal values](../values/temporal.md)'s canonical `date` text |
| `datetime` | [Temporal values](../values/temporal.md)'s canonical `datetime` text |

<a id="req-0733"></a>

**REQ-0733.** An `int` is written without a leading `U+002B`, without digit
grouping, and without a leading zero. Zero is `0`. A `float` that takes no
display precision is written by [Types and conversion](../values/types.md)'s conversion to `str`: the shortest round-
tripping digits in positional notation, with a trailing `.0` omitted. That
conversion has no exponent, which lets this profile promise bytes: two
admissible spellings would leave two conforming runtimes different.

### Writing: Display precision

<a id="req-0744"></a>

**REQ-0744.** `output.decimals` is an optional non-negative integer. It applies
to `csv` alone, and to every `float` column of the artifact.

<a id="req-0745"></a>

**REQ-0745.** When it is absent, a `float` is written as [Types and conversion](../values/types.md)'s float text. When
it is present with the value `n`, a `float` is written in fixed-point form with
exactly `n` digits after the decimal point, and with a decimal point only when
`n` is greater than zero. A value therefore keeps its declared width whether or
not its digits require it: at `n` of 4, an integral 25 is written `25.0000`.

<a id="req-0746"></a>

**REQ-0746.** This is the only place a value is rounded for presentation. It
happens once, when the field is written, and after everything [Execution lifecycle](../execution/lifecycle.md) sequences:
every derivation, every conversion, every verification, key
validation, and row ordering. No dependent column, predicate, aggregate,
verification, key, or order term ever sees a rounded value, and changing
`output.decimals` cannot change whether a run passes or which rows it produces.

#### The rounding is exact and host-independent

<a id="req-0747"></a>

**REQ-0747.** Each binary64 value is an exact decimal fraction. To round the
value, multiply by ten raised to `n`, round the product to an integer with a
tie away from zero, then divide by ten raised to `n`. Write a value that
rounds to zero without a sign.

<a id="req-0748"></a>

**REQ-0748.** The tie is decided on the exact value, never on a shortened
representation of it, and the difference is observable:

| Value as written in source | Its exact binary64 value | `decimals: 2` |
|---|---|---|
| `0.125` | 0.125 | `0.13` |
| `-0.125` | -0.125 | `-0.13` |
| `2.675` | 2.674999999999999822364316059974953532218933105468750 | `2.67` |

<a id="req-0749"></a>

**REQ-0749.** `0.125` is representable, so it is a genuine tie and rounds away
from zero. `2.675` is not representable; its nearest binary64 is below it, so
there is no tie to break and it rounds down. An implementation that first
shortens the value to `2.675` and then rounds reports `2.68` and does not
conform.

<a id="req-0750"></a>

**REQ-0750.** No host rounding or formatting routine may be assumed to do this.
R's and Python's `round` both send an exact tie to the even digit rather
than away from zero, and the C formatting both ecosystems build on does the
same. Each of the three disagrees with this contract on `0.125`, so an
implementation performs the exact scaling above rather than delegating.

### Reading: CSV spellings are admitted only when they carry the same records

<a id="req-0833"></a>

**REQ-0833.** For the `csv` profile, a writer controls its bytes and emits
one spelling, while a reader receives a study file as stored. This contract
admits a second spelling only when both spellings deliver the same records,
and refuses every other difference rather than silently repairing it.

<a id="req-0834"></a>

**REQ-0834.** Admitted: a record terminated by `U+000D U+000A` rather than
`U+000A`, and a final record with no terminator at all.

<a id="req-0835"></a>

**REQ-0835.** Refused: a byte-order mark, a `U+000D` anywhere else, and every
reader option in *Nothing here is configuration*. Each refusal prevents a
change to the records or values the file delivers. Admitting any change would
make two conforming runtimes disagree about the same bytes.

### Reading: Encoding

<a id="req-0836"></a>

**REQ-0836.** A delimited source is UTF-8. Ill-formed encoded bytes fail under
[Text values](../values/text.md) rather than being replaced, skipped, or decoded under a machine default.

<a id="req-0837"></a>

**REQ-0837.** A byte-order mark is rejected rather than skipped. Readers that
skip the mark and readers that keep it disagree about the first field's
name, so a marked file has a header whose first name depends on the reader.
Rejecting the mark names the defect for the producer to fix. Skipping the
mark would accept a file whose header this design cannot state.

### Reading: Records and fields

<a id="req-0838"></a>

**REQ-0838.** A **record terminator** is `U+000A`, optionally preceded by
`U+000D`. The final record may omit its terminator. A file's records are the
same under either terminator, which is why both are admitted.

<a id="req-0839"></a>

**REQ-0839.** `,` separates the fields of a record. It is the only delimiter.

<a id="req-0840"></a>

**REQ-0840.** A field is either **bare** or **quoted**, and the choice is a
property of the stored field rather than of its value:

<a id="req-0841"></a>

**REQ-0841.** A **bare** field runs from the delimiter or terminator before
  it to the one after it. It carries no `U+0022`, and no `U+000D`.

<a id="req-0842"></a>

**REQ-0842.** A **quoted** field opens and closes with `U+0022`. Inside it,
  `U+0022 U+0022` is one literal `U+0022`, and `,` and `U+000A` are ordinary
  characters of the value. A closing quote is followed only by a delimiter or
  a terminator.

<a id="req-0843"></a>

**REQ-0843.** `U+000D` occurs only as the first character of a record
terminator. Inside a quoted field the character fails rather than joining
the value. Without this refusal, a file with `U+000D U+000A` terminators
would deliver a different value than the same file with `U+000A` terminators.
Admitting both terminators is meant to remove exactly that
disagreement.

<a id="req-0844"></a>

**REQ-0844.** Nothing is trimmed. A space beside a delimiter is a character of
the field, and a reader that removes it changes a collected value.

### Reading: The header

<a id="req-0845"></a>

**REQ-0845.** The first record is the **header**, and it names the source's
fields in order. Each name is non-empty, and no two names are the same
under [Text values](../values/text.md)'s equality. A quoted name carries the text inside its quotes, so a
quoted empty name is an empty name and fails.

<a id="req-0846"></a>

**REQ-0846.** Every later record carries exactly as many fields as the header
names. A record with more or fewer fails. Padding a short record and
discarding a long record's surplus are not implementation options. Both
accept a file whose shape the study did not intend.

<a id="req-0847"></a>

**REQ-0847.** A file with no bytes has no header and fails. A header-only
file holds no records, which is valid: an empty dataset still has its fields.

### Reading: Nothing here is configuration

<a id="req-0848"></a>

**REQ-0848.** An implementation must not expose, and must not silently apply, a
reader option that changes what this contract fixes. In particular: no comment
prefix or skipped preamble, no alternate delimiter or quote character, no
whitespace trimming, no header synthesis or renaming, and no missing-value
sentinel.

<a id="req-0849"></a>

**REQ-0849.** A missing-value sentinel is easy to mistake for an oversight.
[Source ingestion](ingestion.md) fixes what a stored field means, including that no text spells absence.
A reader option that spells absence decides the meaning of absence before
any rule in this design sees the value.

### Reading: Quoting is transport, not meaning

<a id="req-0850"></a>

**REQ-0850.** Every field reaches [Source ingestion](ingestion.md) as its text or as missing, and an
implementation must preserve the text and the missing state. A field with
no characters is missing, bare or quoted. Quoting is a transport detail the
reader does not report. [Source ingestion](ingestion.md) gives an empty field one meaning and never sees
an empty string. Common dataframe readers discard text-versus-missing
distinctions by default; conformance is a property of what the reader
delivers, not of which library produced it.

## Error conditions

### Writing: Errors

<a id="req-0761"></a>

**REQ-0761.** An
`output.decimals` that is not a non-negative integer: fail validation.

<a id="req-0762"></a>

**REQ-0762.** An `output.decimals` declared on a path the mapping resolves to
`parquet`: fail validation with `decimals_not_applicable`. A display precision
that cannot take effect is a defect in the specification rather than a setting
to ignore.

<a id="req-0765"></a>

**REQ-0765.** Writing a byte-order
mark, a `U+000D` record terminator, or a quoting that differs from the `csv`
condition: none is an implementation option.

<a id="req-0766"></a>

**REQ-0766.** Rounding with a host
routine whose ties do not go away from zero, or rounding a value another stage
can observe: neither is an implementation option.

### Reading: Errors

<a id="req-0851"></a>

**REQ-0851.** A failure names the input dataset, the path exactly as the
specification wrote it, and the record and field where it was decided.
Records and fields are counted from one, and the header is record one. A
message carries no host path, for the reason [Resource resolution](resources.md) gives.

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

<a id="req-0853"></a>

**REQ-0853.** Ill-formed encoded bytes: fail with [Text values](../values/text.md)'s `invalid_text`,
  reporting the record and field the reader had reached.

<a id="req-0854"></a>

**REQ-0854.** Repairing a rejected file in the reader -- skipping a mark,
  trimming a field, padding a record, renaming a duplicate name, or
  normalizing a terminator inside a value: none is an implementation option.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-source-field-duplicate](../../benchmarks/negative-source-field-duplicate/README.md).
- [negative-source-unnamed-field](../../benchmarks/negative-source-unnamed-field/README.md).
- [negative-source-extra-field](../../benchmarks/negative-source-extra-field/README.md).
- [negative-source-unterminated-quote](../../benchmarks/negative-source-unterminated-quote/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Read admitted CSV spellings and write canonical CSV bytes and display precision. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
