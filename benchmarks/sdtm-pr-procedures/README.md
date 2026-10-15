# Prior Procedures from Surgery and Radiotherapy Forms

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-pr-procedures.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Procedures (PR) record from each of two
prior-procedure forms carried in a single ODM item extract:
a prior cancer surgery form and a prior radiotherapy form.
`PRTRT` keeps the reported procedure name, `PRCAT` names the
form, `PRLOC`/`PRLAT` carry the surgery site, and the
radiotherapy record carries its dose in `PRDOSE`/`PRDOSU`.
`PRSTDTC`/`PRENDTC` keep the precision collected, so a
year-month surgery date stays `2023-11`. The pre-specified
question "Any prior radiotherapy?" always writes a record:
`PRPRESP` is `Y`, and `PROCCUR` is `Y` or `N`.

**Input:** one ODM file (`input/odm.csv`) with one row per
collected item value, keyed by study, subject, study event,
form and item. Subject 001 reports a lumpectomy with only a
year-month date and a radiotherapy course of 50 Gy from
2024-02-10 to 2024-03-20. Subject 002 answers "No" to prior
radiotherapy, so only the question and its answer are
collected for that subject.

**Variables:**

- `PRSEQ` numbers each subject's records by `PRSTDTC` and
  then by `PRTRT`; with `STUDYID` and `USUBJID` it identifies
  the record.
- `PRCAT` names the form: `PRIOR CANCER SURGERY` or
  `PRIOR RADIOTHERAPY`.
- `PRPRESP` is `Y` for the pre-specified radiotherapy
  question and blank for the reported surgery.
- `PROCCUR` is `Y` or `N` for the pre-specified question
  and blank for the reported surgery.
- `PRLOC` and `PRLAT` carry the surgery site; they are blank
  for radiotherapy.
- `PRDOSE` and `PRDOSU` carry the radiotherapy dose; they are
  blank when the procedure did not occur and for surgery.
- `PRSTDTC` and `PRENDTC` keep the collected precision: the
  lumpectomy date stays `2023-11`, while the radiotherapy
  course carries full start and end dates.

**Note:** a "No" answer is still a record. It lets the study
tell "no prior radiotherapy" apart from "never asked", and it
carries no dose and no dates.

Provenance: `PRPRESP`/`PROCCUR` follow the SDTMIG
pre-specified pattern (an asked question with an occurrence
flag). All subjects, procedures and dates are invented
fixtures.

**Standard:** SDTM | **Domain:** PR
