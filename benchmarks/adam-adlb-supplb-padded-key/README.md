# Join a Numeric Sequence to a Padded Supplemental Key

**Goal:** carry the supplemental qualifier value onto each analysis
laboratory row (`QVAL`, `QVAL_NAMED`).

**Input:** laboratory records carry a numeric sequence number; supplemental
records carry the same sequence as eight-character, space-padded text. Two
subjects may share a sequence number.

**Variables:**

- `QVAL` is the qualifier value from the supplemental record whose study,
  subject, and padded sequence all match; empty when nothing matches.
- `QVAL_NAMED` holds the same value from the same match, taken again once
  every laboratory row exists; empty when nothing matches.

**Note:** the padding must be exact: a zero-padded key does not match the
space-padded form, and a supplemental record with no laboratory record
changes nothing. Rows in both sequence ranges use the same match.
