# Parent Linkage

**Goal:** create one supplemental record per collected qualifier,
carrying `IDVARVAL`, `QLABEL`, `QVAL`, `QORIG`, and `QEVAL`.

**Input:** two tables sharing study and subject identifiers: a
parent medical history (MH) table with one record per condition,
and a qualifier table with one record per condition carrying the
two collected qualifiers.

**Variables:**

- `IDVARVAL` is the parent sequence number as text, matched on
  subject and condition term.
- `QLABEL` is the qualifier label: `Family History` for `MHFAMHX`,
  `Confirmed by Medical Records` for `MHCONF`.
- `QVAL` is the collected answer, `Y` or `N`.
- `QORIG` is the origin, always `Collected` (Define-XML 2.1) for
  collected values.
- `QEVAL` is blank, since a collected value is not an assessment.

**Note:** a qualifier that finds no parent record is an error
rather than a record with an empty link, so every supplemental
record points at a real parent. A parent record with no collected
qualifiers simply contributes nothing.
