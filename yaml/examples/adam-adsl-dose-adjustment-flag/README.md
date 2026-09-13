# Dose adjustment flag from three sources

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-dose-adjustment-flag.html)

**Goal:** one record per subject flagging any reported dose
adjustment (`DOSADJFL`).

**Input:** subject list plus exposure (EX) records carrying
`EXADJ`, exposure as collected (EC) records carrying `ECADJ`,
and findings about (FA) records carrying `FATESTCD`, `FAOBJ`,
and `FASTRESC`.

**Variables:**

- `DOSADJFL` holds `Y` when any source holds a qualifying
  record: a filled `EXADJ` value, a filled `ECADJ` value, or a
  findings about row with `FATESTCD` of `OCCUR`, `FAOBJ` of
  `DOSE ADJUSTMENT`, and `FASTRESC` of `Y`. It holds `N` when
  the subject has at least one record in any of the three
  sources but no qualifying record. It stays empty when the
  subject is absent from all three sources.

**Note:** a qualifying record takes precedence over other
records, matches use the study and subject identifiers, and a
source record without a matching subject adds no record.

**Standard:** ADaM | **Domain:** ADSL
