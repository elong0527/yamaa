# Supplemental Qualifier Metadata by Qualifier Name

**Goal:** build the supplemental demographics qualifiers `QNAM`, `QLABEL` and
`QVAL` for each subject's additional races and other-race text, and describe
each qualifier's values separately in the data-definition document.

**Input:** one collected record per subject with up to two races and the
free text a site entered for a race not on the list.

**Variables:**

- `RDOMAIN` is `DM`, the domain the qualifiers belong to.
- `IDVAR` and `IDVARVAL` are missing: a demographics qualifier belongs to the
  subject rather than to one record.
- `QNAM` is `RACE1` or `RACE2` for the first and second reported race, and
  `RACEOTH` for the other-race text.
- `QLABEL` is the qualifier's label: `Race 1`, `Race 2` or `Race, Other`.
- `QVAL` is the reported value: a race from the race codelist for `RACE1` and
  `RACE2`, and the text exactly as entered for `RACEOTH`.
- `QORIG` is `Collected`, since every value comes from the form.
- `QEVAL` is missing: no one evaluated the values.

**Note:** a subject has a qualifier record only for a race or text actually
reported. The data-definition document describes `QVAL` once for the two race
qualifiers, drawn from the race codelist and collected on page 2 of the
annotated form, and once for the other-race text, collected on page 3. Each
description gives the longest value of its own records, so the other-race
text is shorter than the variable.
