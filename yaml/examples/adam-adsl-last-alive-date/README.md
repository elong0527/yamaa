# Last known alive date from several sources

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-last-alive-date.html)

**Goal:** derive `LSTCNTDT` and `LSTALVDT` for each subject from the
contact, treatment, adverse event, and vital signs dates.

**Input:** subject-level records carrying `TRTEDT` and the collected
contact text `LSTCNTDC`, alongside adverse event end dates (`AENDT` in
ADAE) and vital signs dates (`ADATE` in ADVS).

**Variables:**

- `LSTCNTDT` is the contact text completed to a day: a year and month
  take the last day of that month, a year alone takes the last day of
  December, and missing or unusable text leaves the date missing.
- `LSTALVDT` is the latest of `TRTEDT`, `LSTCNTDT`, the subject's
  latest `AENDT`, and the subject's latest `ADATE`. A completed date
  competes on the day it names. When every source is missing the date
  stays missing; otherwise the latest available date is kept.

**Note:** a contact collected as `2025-02` completes to `2025-02-28`,
which beats every date collected in full, while `2025` alone becomes
`2025-12-31`. A contact collected in full can still lose: an adverse
event ending one day later is kept instead.

**Standard:** ADaM | **Domain:** ADSL
