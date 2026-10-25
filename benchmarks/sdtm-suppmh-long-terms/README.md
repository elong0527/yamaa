# Continue Long Medical History Terms Between Words

**Goal:** write the supplemental records `MHTERM1` and `MHTERM2` that
continue a reported medical history term longer than 200 characters,
carrying `IDVARVAL`, `QLABEL`, `QVAL`, `QORIG`, and `QEVAL`.

**Input:** a medical history extract with one record per condition,
carrying the parent record sequence and the reported term as collected.

**Variables:**

- `IDVARVAL` is the parent record sequence, written as text.
- `QNAM` is `MHTERM1` for the first continuation and `MHTERM2` for the
  second.
- `QLABEL` is `Reported Term for the Medical History 1` for `MHTERM1` and
  `Reported Term for the Medical History 2` for `MHTERM2`.
- `QVAL` is the piece of the reported term that the record continues.
- `QORIG` records the collected origin as `Collected`.
- `QEVAL` is blank, since a collected value is not an assessment.

**Note:** the reported term is split into pieces of at most 200
characters, each ending after the last whole word that fits, so no word is
broken across two pieces; the space at each break belongs to neither piece.
The first piece stays on the medical history record itself. A term of 200
characters or fewer has no supplemental record, a term with two pieces has
`MHTERM1` only, and a term with three has both; a term that needs a fourth
piece stops the run instead of losing its end.
