# Link separately collected qualifiers to parent records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-suppmh-parent-linkage.html)

**Goal:** create one supplemental record per collected qualifier,
carrying `IDVARVAL`, `QLABEL`, `QVAL`, `QORIG`, and `QEVAL`.

**Input:** two tables sharing study and subject identifiers: a
parent medical history (MH) table with one record per condition,
and a qualifier table with one record per condition carrying the
two collected qualifiers.

**Variables:**

- `IDVARVAL` is the parent sequence number as text, matched on
  subject and condition term.
- `QLABEL` is the qualifier label, `Family History` or `Confirmed
  by Medical Records`.
- `QVAL` is the collected answer, `Y` or `N`.
- `QORIG` is the origin, always case report form (`CRF`) for
  collected values.
- `QEVAL` is blank, since a collected value is not an assessment.

**Note:** a qualifier that finds no parent record is an error
rather than a record with an empty link, so every supplemental
record points at a real parent. A parent record with no collected
qualifiers simply contributes nothing.

**Standard:** SDTM | **Domain:** SUPPMH
