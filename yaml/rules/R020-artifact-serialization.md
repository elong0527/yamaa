---
id: R020
title: Artifact Serialization
status: normative
applies_to: [root.output, output.path, output.decimals, output.violation_log]

---

# Artifact serialization

## Intent

Define how a completed, ordered primary artifact and any R009 violation log
become bytes. This rule specifies containers, value text, missing and empty
strings, float display precision, and target replacement.

## Boundaries

This rule starts where R005 ends. R005 owns the artifact's columns, their
order, its rows, and row order. R020 cannot change them. R011 owns column
values and the text conversion to `str` produces. R020 owns the one display
rounding after every calculation. R016 owns the canonical text of a `date` and
`datetime`. R019 owns string contents, ill-formed-text failure, and string
order.

R014 owns the other direction. R014 states what a stored field means when a
specification reads the field. `csv` below is the writing counterpart of the
delimited form R014 reads. The two agree on missing. The two part on the empty
string. Neither restates the other. R020-17 states where the two part.
R023 owns the syntax a specification reads a delimited *source* under, and
admits the spellings a reader receives that this rule never writes. R027 is
the reading counterpart of the `parquet` profile and applies the inverse of
this rule's type mapping.

This rule owns which file a specification declares and produces, the bytes
the file receives, and the replacement of the file. It does not own how that
path is resolved against a project or which locations a run may write to: R002
owns resolution and containment for the paths a specification names, and an
artifact path is written, so a boundary that admits a source does not by
itself admit a target.

## The artifact's path selects its profile

**R020-1.** `output.path` names the primary file the specification produces. It
is required: a specification that derives an artifact says what it produces,
and there is no default name for one. `output.violation_log` names R009's
sidecar when the specification declares one.

**R020-2.** The path's extension selects the profile. The mapping is closed, so
an extension outside it names no profile and fails validation rather than
falling back to one. The extension is matched without regard to case, because a
study that stores `ADSL.CSV` names the same container as one that stores
`adsl.csv` and two runtimes must not disagree about which.

| Extension | Profile | Container | What two runtimes must agree on |
|---|---|---|---|
| `.csv` | `csv` | delimited text | the bytes |
| `.parquet` | `parquet` | Parquet | schema, column/row order, values |

**R020-3.** One field carries both facts: a specification that produces
an artifact needs a path regardless. A separate profile beside it could
disagree with the name it writes -- an `adsl.csv` declared `parquet` is a file
whose name lies about its contents. The cost is stated rather than hidden:
renaming the artifact changes the container, so a rename is a change to the
contract and not only to a filename.

**R020-4.** Deriving is not guessing. The extension is read from the
specification, where a reviewer sees it, against a closed mapping this rule
fixes; an unrecognized extension stops the run. A reader that instead sniffed a
file's contents, or accepted an unknown extension under a default, could read a
conforming artifact wrongly without failing, and neither is permitted.

**R020-5.** The two profiles exist for different readers. `parquet` is the
production container: it carries its own types, so an artifact read by another
specification needs no declaration to be understood, and a large one does not
pay for decimal text. `csv` is the reviewable container: a human can read it, a
diff can show what moved in it, and its bytes are fixed exactly, which is what
makes it usable as a golden contract.

**R020-6.** A profile and the specification's `schema_version` identify the
bytes exactly. A consumer receives both because R014's producing-specification
link carries the whole producer document, not only the profile.

**R020-7.** A later release that changes any byte-level or mapping decision
below therefore changes what a profile means at that schema version, and an
artifact keeps the meaning its producer's version gives it. A profile that ever
has to diverge from the schema version is added as a name rather than by
redefining one of these two.

## The csv profile

### Bytes

- **R020-8.** The artifact is UTF-8 and has no byte-order mark. R019
  owns the text being encoded.
- **R020-9.** `U+000A` terminates every record, including the last, so every
  artifact ends with it. `U+000D` is never written as part of a terminator; it
  appears only inside a quoted field that contains one.
- **R020-10.** `U+002C` separates fields. No other delimiter is defined.
- **R020-11.** The first record is the header: `output.columns` names, in
  that order, written under the quoting rule below.
- **R020-12.** Each following record is one row, in the order R005 fixes,
  holding one field per header name in the same order.
- **R020-13.** An artifact with no rows is the header record and its terminator
  alone. It is not empty, because the columns exist whether or not a row
  does.

### Quoting

**R020-14.** A field is quoted exactly when its text contains `U+0022`,
`U+002C`, `U+000D`, or `U+000A`, or when the field is the empty string. Every
other field is written bare.

**R020-15.** A quoted field is wrapped in `U+0022` and each `U+0022` within its
text is written twice. Nothing else is escaped: a quoted field carries its
newlines, delimiters, and every other scalar exactly.

**R020-16.** The exact condition makes both runtimes quote the same fields. A
writer that quotes a field the condition leaves bare, or leaves bare a field
the condition quotes, does not conform even if ordinary readers accept its
output.

### Missing and the empty string

**R020-17.** A missing value is written as no characters at all, unquoted. A
collected empty string is written as two quote characters. The two forms stay
apart in the artifact, and no text is ever pressed into service as a sentinel
for absence. Reading does not restore the pair: R014-16 reads an empty field as
missing whether it was bare or quoted, so a collected empty string written here
returns as missing if this artifact is later read as a delimited source. The
asymmetry is deliberate. A source is authored by a producer this language does
not control. A distinction no such producer reliably spells is not one a
reader may invent. An artifact this rule writes has one writer and can
afford the finer form. The `parquet` profile carries the pair in its container
and keeps it in both directions.

    STUDYID,COMMENT,NOTE
    S1,plain text,
    S1,"has, comma",""
    S1,"say ""hi""",x

The third row's `NOTE` is the ordinary string `x`. The second row's is a
collected empty string. The first row's is missing.

### Value text

**R020-18.** Value text by column type:

| Column type | Text written |
|---|---|
| `str` | its scalar values, under R019 |
| `int` | its decimal digits, with a leading `U+002D` when negative |
| `float` | R011's float text, or the fixed-point form below |
| `date` | R016's canonical `date` text |
| `datetime` | R016's canonical `datetime` text |

**R020-19.** An `int` is written without a leading `U+002B`, without digit
grouping, and without a leading zero. Zero is `0`. A `float` that takes no
display precision is written by R011's conversion to `str`: the shortest round-
tripping digits in positional notation, with a trailing `.0` omitted. That
conversion has no exponent, which lets this profile promise bytes: two
admissible spellings would leave two conforming runtimes different.

## The parquet profile

### Column mapping

**R020-20.** Each declared type maps to exactly one Parquet physical/logical
type.

| Column type | Physical | Logical |
|---|---|---|
| `str` | `BYTE_ARRAY` | `String` |
| `int` | `INT64` | none |
| `float` | `DOUBLE` | none |
| `date` | `INT32` | `Date` |
| `datetime` | `INT64` | `Timestamp`, microseconds, not adjusted to UTC |

**R020-21.** The schema's fields are the names in `output.columns`, in that
order. Every field is optional, because every column type admits a missing
value.

### Missing and the empty string

**R020-22.** A missing value is a Parquet null. A collected empty string is a
present `BYTE_ARRAY` of zero length, which is not null. This is the same
distinction `csv` draws between a bare field and two quote characters, carried
by the container instead of by a convention.

### Temporal values

**R020-23.** A `date` is days from 1970-01-01, and a `datetime` is the
count of microseconds from 1970-01-01T00:00:00 on the same wall clock the value
names.

**R020-24.** R016's `datetime` is a reading on a wall clock and carries no zone
and no offset. Its Timestamp is not adjusted to UTC. An implementation must
not attach a zone when writing or reading. A runtime whose
native timestamp always carries a zone -- R016 names R's `POSIXct` as such a
type -- must still write and read this column so that the same wall clock
survives. Shifting a value into or out of a machine timezone changes the
value. Two runtimes that each shift by their own offset do not agree.

**R020-25.** A `datetime` has whole-second resolution, so its microsecond part
is always zero. Microseconds are used because the format offers no second unit
and because both ecosystems' readers agree on this one; the finer resolution is
never used.

### Determinism

**R020-26.** Two runtimes writing the same completed dataset must produce
Parquet artifacts that read back identically: the same field names in the same
order, the same logical types, the same rows in the same order, the same nulls,
and the same values, with every `DOUBLE` bit-identical.

**R020-27.** An implementation writes uncompressed pages and adds no key-value
metadata of its own beyond what the format requires.

**R020-28.** The bytes are not fixed. A Parquet writer stamps its own
identity and version into the file, and the row-group and page sizing, the
encodings it selects, and the statistics it records are properties of the
library rather than of this design. Requiring identical bytes would require
every conforming implementation to abandon its ecosystem's writer, which buys
less than it costs. An artifact needing direct byte comparison is written
under `csv`, whose byte guarantee is exactly that.

### Floats are stored, not rendered

**R020-29.** A `float` enters this profile as the binary64 value its derivation
produced. `output.decimals` does not apply. No rounding happens on output, so a
consumer that reads the artifact receives the value the calculation used.
Storing a container's native double is not display. This design rounds once,
at display.

## Display precision

**R020-30.** `output.decimals` is an optional non-negative integer. It applies
to `csv` alone, and to every `float` column of the artifact.

**R020-31.** When it is absent, a `float` is written as R011's float text. When
it is present with the value `n`, a `float` is written in fixed-point form with
exactly `n` digits after the decimal point, and with a decimal point only when
`n` is greater than zero. A value therefore keeps its declared width whether or
not its digits require it: at `n` of 4, an integral 25 is written `25.0000`.

**R020-32.** This is the only place a value is rounded for presentation. It
happens once, when the field is written, and after everything R005 sequences:
every derivation, every conversion, every verification, key
validation, and row ordering. No dependent column, predicate, aggregate,
verification, key, or order term ever sees a rounded value, and changing
`output.decimals` cannot change whether a run passes or which rows it produces.

### The rounding is exact and host-independent

**R020-33.** Each binary64 value is an exact decimal fraction. To round the
value, multiply by ten raised to `n`, round the product to an integer with a
tie away from zero, then divide by ten raised to `n`. Write a value that
rounds to zero without a sign.

**R020-34.** The tie is decided on the exact value, never on a shortened
representation of it, and the difference is observable:

| Value as written in source | Its exact binary64 value | `decimals: 2` |
|---|---|---|
| `0.125` | 0.125 | `0.13` |
| `-0.125` | -0.125 | `-0.13` |
| `2.675` | 2.674999999999999822364316059974953532218933105468750 | `2.67` |

**R020-35.** `0.125` is representable, so it is a genuine tie and rounds away
from zero. `2.675` is not representable; its nearest binary64 is below it, so
there is no tie to break and it rounds down. An implementation that first
shortens the value to `2.675` and then rounds reports `2.68` and does not
conform.

**R020-36.** No host rounding or formatting routine may be assumed to do this.
R's and Python's `round` both send an exact tie to the even digit rather
than away from zero, and the C formatting both ecosystems build on does the
same. Each of the three disagrees with this rule on `0.125`, so an
implementation performs the exact scaling above rather than delegating.

## A stored artifact carries its profile

**R020-37.** A specification that reads an artifact another specification
produced learns how those bytes are encoded from the producer, through the
producing specification link R014 defines: the producer's `output.path` states
the profile by its extension, just as `output.columns` states the fields.
The consumer reads the profile from the producing specification, not from
the name the consumer happens to know the file by. A copy stored under
another name is still read under the profile its producer wrote it with.

## Publication

**R020-38.** An artifact becomes visible in one step. An implementation:

1. writes the complete artifact into a temporary regular file in the same
   directory as the target;
2. flushes and closes that file, so its bytes reach the filesystem, not a
   buffer; and
3. atomically replaces the target with it.

**R020-39.** The temporary file is regular and is in the target's own
directory so that the replacement stays within one filesystem and remains
atomic. The name is not fixed and must not collide with the target or with
another run's temporary file.

**R020-40.** A run that fails at any point leaves the target as it was and
removes its temporary file, so a failure produces neither an accepted artifact
nor residue. A reader observes either the artifact that was there before or the
complete new one, and never a prefix of the new one.

**R020-41.** Publication happens once, after the whole artifact is complete:
after every value's lifecycle, key validation, and verification under R005, and
after its rows are ordered. Rows are not streamed to the target as they are
constructed, because a partially constructed dataset is not yet ordered and a
run that fails midway would already have published part of it.

## Rationale

Two containers serve two readers: `parquet` carries types for production use,
while `csv` fixes bytes exactly so diffs stay reviewable and golden contracts
compare byte for byte. Display rounding happens once, after everything a run
decides, so changing `output.decimals` can never change pass or fail. The tie
rule operates on the exact binary value because the nearest double is often not
the decimal written in source, and host rounding routines in both ecosystems
break ties to even rather than away from zero. Atomic publication through a
same-directory temporary file keeps readers from ever observing a partial
artifact.

## Errors

**R020-42.** A missing `output.path`: fail validation and report the
specification. **R020-43.** An `output.path` whose extension is outside the
mapping above, or none: fail validation with `unknown_artifact_profile`
and report the path. No extension is treated as a default. **R020-44.** An
`output.decimals` that is not a non-negative integer: fail validation.
**R020-45.** An `output.decimals` declared on a path the mapping resolves to
`parquet`: fail validation with `decimals_not_applicable`. A display precision
that cannot take effect is a defect in the specification rather than a setting
to ignore. **R020-46.** A value that cannot be written under its column's
mapping: fail and report the column, the row's key, and the value. **R020-47.**
A failed atomic replacement: fail and report the target. The run produces no
artifact, and the previous one is unchanged. **R020-48.** Writing a byte-order
mark, a `U+000D` record terminator, or a quoting that differs from the `csv`
condition: none is an implementation option. **R020-49.** Rounding with a host
routine whose ties do not go away from zero, or rounding a value another stage
can observe: neither is an implementation option.

## Violation-log publication

**R020-50.** `output.path` and `output.violation_log` must differ. Reusing one
path fails validation with `artifact_path_collision` and reports both fields.
Each path's extension independently selects its profile under R020-2; the
primary may be Parquet while its log is CSV, or the reverse.

**R020-51.** When a successful run has a violation log, a publisher renders
and validates both complete artifacts before touching either target. It
publishes the log first and the primary artifact last, applying R020-38 through
R020-40 to each file. A successful publication therefore never exposes a new
primary artifact without its completed log already visible.

**R020-52.** Warning violations do not prevent publication. Failure to render
or replace the log is an output failure, not a warning: the primary target is
not touched. If replacing the primary fails after the log was replaced, the
publication fails and the prior primary remains; the complete log may remain as
the record of the completed candidate run. Atomic replacement is guaranteed per
file, not simultaneously across two paths.
