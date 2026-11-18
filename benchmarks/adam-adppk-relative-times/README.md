# Times Since the First and the Previous Dose on Every Event Record

**Goal:** give every dose record and every pharmacokinetic (PK)
concentration record of a population PK dataset its actual time in hours
since the subject's first dose (`AFRLT`) and since the previous dose
(`APRLT`), with the first dose date/time (`FANLDTM`) and the amount of the
dose the record refers to (`DOSEA`).

**Input:** exposure (EX) records, one per administration, with the dose
(`EXDOSE`) and its date/time (`EXSTDTC`), and PK concentration (PC)
records with the collection date/time (`PCDTC`). An administration gives a
dose record when its dose is above zero, and every sample gives a
concentration record.

**Variables:**

- `ASEQ`: the record's number within the subject, in the order of `ADTM`,
  a sample before a dose at the same date/time.
- `ADTM`: the date/time of the administration or of the sample.
- `EVID`: 1 for a dose record and 0 for a concentration record.
- `AMT`: the dose given; empty on a concentration record.
- `FANLDTM`: the date/time of the subject's earliest dose above zero;
  empty when the subject has none.
- `AFRLT`: hours from `FANLDTM` to `ADTM`, not rounded; negative before
  the first dose; empty when `FANLDTM` is empty.
- `APRLT`: 0 on a dose record. On a concentration record, hours from the
  previous dose to `ADTM`, not rounded, where the previous dose is the
  latest dose before the sample; a sample taken at the time of a dose was
  taken before it, so it refers to the dose before. A sample with no dose
  before it has the value of `AFRLT`.
- `DOSEA`: on a dose record, its own dose. On a concentration record, the
  amount of the previous dose, or of the first dose for a sample with no
  dose before it; empty when the subject has no dose.

**Note:** an administration with a dose of zero is never a first or a
previous dose, and date/times are clock readings taken as written, with no
time zone.
