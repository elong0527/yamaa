# Subject elements from the trial design

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-se-subject-elements.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Subject Elements (SE) record per subject per element
the subject entered, carrying `SESEQ`, `ETCD`, `ELEMENT`, `TAETORD`,
`EPOCH`, `SESTDTC`, `SEENDTC`, `SESTDY`, `SEENDY`, and `SEUPDES`.

**Input:** the planned element sequence per arm from the trial arms table,
element definitions and timing rules from the trial elements table, one
demography record per subject with arm and reference dates, daily dosing
records, and the planned subject-by-element list joining them.

**Variables:**

- `SESEQ` numbers the subject's elements in planned order, from the trial
  arms table.
- `ETCD` is the element code from the trial elements table.
- `ELEMENT` is the element description from the trial elements table.
- `TAETORD` is the planned order of the element within the subject's arm.
- `EPOCH` is the epoch the element belongs to, from the trial arms table.
- `SESTDTC` is the date the subject actually started the element: informed
  consent for screening, first dosing date for treatment, last dosing date
  for follow-up.
- `SEENDTC` is the date the subject actually ended the element: first dosing
  date for screening, last dosing date for treatment, end of study
  participation for follow-up.
- `SESTDY` is the study day of the element start against the reference
  start date.
- `SEENDY` is the study day of the element end against the reference
  start date.
- `SEUPDES` is blank here; it only carries a description when the subject
  entered an element outside the planned sequence.

**Note:** elements are recorded back to back: an element ends on the day the
next one starts. The treatment element ends on the actual last dosing date,
so a subject who stopped dosing early has a shorter treatment element and an
earlier follow-up start than the plan described.

Provenance: the trial design, subjects, dates, and dosing records are
invented fixtures.

**Standard:** SDTM | **Domain:** SE
