# Event Findings

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-fa-event-findings.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `FASEQ`, `FATESTCD`, `FATEST`, `FAOBJ`, `FACAT`,
`FAORRES`, `FAORRESU`, `FASTRESC`, `FASTRESN`, `FASTRESU`,
`FALNKID`, and `FADTC` for findings collected about individual
adverse events on a supplementary form, in Findings About (FA).

**Input:** long-form Operational Data Model (ODM) item data: one row per
collected item, identified by study (`StudyOID`), subject (`SubjectKey`),
event (`StudyEventOID` plus its repeat key), item group, and item
(`ItemOID`), with the collected answer in `Value`. Each adverse event is one
event occurrence, and the findings collected about it ride on the same
occurrence. The findings form's `IT.FA.AELNKID` item carries the link
identifier shared with the adverse event record, whose term is the
`IT.AE.AETERM` item on that same occurrence. The study and subject
identifiers come straight from the ODM rows; no demographics input is
needed.

**Variables:**

- `FASEQ` numbers the subject's records by linked event, then by test
  order (`LOC`, `SIZE`, `BIOPSY`); the collected result breaks any
  remaining ties so the numbering is fully determined.
- `FATESTCD` is `LOC` for the location record, `SIZE` for the size
  record, and `BIOPSY` for the biopsy record. A test with no answer on
  the form has no record.
- `FATEST` is `Location`, `Size`, and `Biopsied` respectively.
- `FAOBJ` is the event term of the linked adverse event, read from the
  `IT.AE.AETERM` item on the same event occurrence. Every event occurrence
  must carry that term: one without it, such as a findings form with no
  matching event, fails the run instead of writing a record with no event
  term.
- `FACAT` is always `AE`.
- `FAORRES` is the answer as recorded on the form.
- `FAORRESU` is the size unit, on `SIZE` records only.
- `FASTRESC` copies `FAORRES`.
- `FASTRESN` is the measured size as a number, on `SIZE` records
  only.
- `FASTRESU` is the size unit, on `SIZE` records only.
- `FALNKID` carries the link identifier shared with the linked
  adverse event record, tying each finding to its specific event.
- `FADTC` is the date the findings form was collected.

**Note:** an adverse event with no findings form has no FA records. Two
events can share the same term, so `FAOBJ` alone does not distinguish them;
`FALNKID` is what ties each finding to its specific event record.

**Standard:** SDTM | **Domain:** FA
