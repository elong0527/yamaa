# Carry forward a once-measured characteristic

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-once-measured-carry-forward.html)

**Goal:** derive `ADT`, `AVAL`, `TRTSDT`, and `HEIGHTBL` for each
planned measurement: height is planned once under the code
`HEIGHT` and weight is planned repeatedly under the code
`WEIGHT`.

**Input:** a planned-measurement spine per subject carrying the
planned order, the parameter code, and `ADT`; long-form vital
signs carrying the test code (`VSTESTCD`), the collection date
(`VSDTC`), the numeric result (`VSSTRESN`) that supplies the
analysis value, and the sequence number; and subject treatment
dates (`TRTSDT`). A collected record belongs to a planned
measurement when the test code matches the planned parameter and
the collection date matches `ADT`.

**Variables:**

- `ADT`: the planned analysis date, carried through unchanged.
- `AVAL`: the collected numeric result when the planned
  measurement was collected, otherwise the most recent earlier
  collected value for the same subject and parameter; missing
  before the first collected value.
- `TRTSDT`: the subject treatment start date, repeated on every
  record of the subject.
- `HEIGHTBL`: the latest height collected on or before treatment
  start, repeated on every record of the subject so later weight
  records retain the once-measured characteristic; missing for a
  subject with no pre-treatment height.

**Note:** a carried value never crosses subjects or parameters,
while `HEIGHTBL` repeats one selected height across both
parameters.

**Standard:** ADaM | **Domain:** ADVS
