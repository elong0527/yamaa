# Tell an inapplicable compartment from an uncollected sample

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-lb-conditional-compartments.html)

**Goal:** build one record per skin compartment a subject has for
the test code (`IL13`), carrying `LBTESTCD`, `LBTEST`, `LBSPEC`,
`LBLOC`, `LBORRES`, `LBORRESU`, `LBSTRESN`, and `LBSTAT`.

**Input:** one row per subject carrying cohort, lesional result,
non-lesional result, and unit.

**Variables:**

- `LBTESTCD` is `IL13` on every record.
- `LBTEST` is `Interleukin 13` on every record.
- `LBSPEC` is `SKIN` on every record.
- `LBLOC` is `LESIONAL` on the lesional record and `NON-LESIONAL`
  on the non-lesional record; the lesional record is built only
  when the cohort is not `NONAD`, while every subject gets the
  non-lesional record.
- `LBORRES` is the collected result as reported, taken from the
  lesional result on lesional records and from the non-lesional
  result on non-lesional records; blank when the expected sample
  was not analysed.
- `LBORRESU` is the collected unit, taken on every record,
  including records whose result is blank.
- `LBSTRESN` is the numeric form of the reported result; missing
  when the reported result is blank.
- `LBSTAT` is `NOT DONE` when the reported result is blank; blank
  otherwise.

**Note:** a blank result with `NOT DONE` means the compartment
exists but its sample was not analysed, while a missing lesional
record means the subject has no lesional compartment.

**Standard:** SDTM | **Domain:** LB
