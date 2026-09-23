# Subject elements from an ODM extract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-se-subject-elements.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Subject Elements (SE) record per subject per element
the subject entered, carrying `SESEQ`, `ETCD`, `ELEMENT`, `TAETORD`,
`EPOCH`, `SESTDTC`, `SEENDTC`, `SESTDY`, `SEENDY`, and `SEUPDES`.

**Input:** a long-format Operational Data Model (ODM) extract — one row per
subject per study event per collected item — carrying each element's
trial-design attributes (description, planned order, epoch) together with
the subject-level reference dates and first and last dosing dates.

**Variables:**

- `SESEQ` numbers the subject's elements in planned order.
- `ETCD` is the element code, taken from the subject's study event.
- `ELEMENT` is the element description, e.g. Screening.
- `TAETORD` is the planned order of the element within the subject's arm.
- `EPOCH` is the epoch the element belongs to, e.g. TREATMENT.
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
earlier follow-up start than the plan described. The extract repeats each
subject's trial-design attributes and reference dates on every study event,
so each SE record derives from its own event context without reading across
events.

Provenance: the trial design, subjects, dates, and dosing dates are invented
fixtures.

**Standard:** SDTM | **Domain:** SE
