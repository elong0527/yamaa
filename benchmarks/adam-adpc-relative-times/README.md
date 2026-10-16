# Actual Times Since the First Dose and Since the Reference Dose

**Goal:** place each pharmacokinetic (PK) concentration sample on two
actual time scales in hours: the time elapsed since the first dose of the
drug the analyte measures (`AFRLT`) and since the sample's reference dose
(`ARRLT`), with the sample, first-dose and reference-dose date/times
(`ADTM`, `FANLDTM`, `PCRFTDTM`).

**Input:** PK concentration (PC) records carrying the analyte
(`PCTESTCD`), the planned time point (`PCTPT`) and the collection
date/time (`PCDTC`), and exposure (EX) records carrying the treatment
(`EXTRT`) and the dose start date/time (`EXSTDTC`). Each concentration
record gives one output record, identified by its sequence number
(`PCSEQ`); the records of a subject who was never dosed stay.

**Variables:**

- `PARAMCD` and `ATPT`: the analyte and the planned time point, as
  collected.
- `ADTM`: the collection date and time; empty when the sample was recorded
  with a date but no time, which is never filled in.
- `FANLDTM`: start of the subject's earliest dose of the analyte's drug,
  `DRUG X` for the parent drug (`DRGX`) and its metabolite (`MET1`) and
  `DRUG Y` for `DRGY`; empty when the subject has no dose of that drug.
- `PCRFTDTM`: start of the reference dose, the latest dose of that drug
  starting at or before the sample, so a sample taken at a dose time refers
  to that dose; for a sample taken before every dose, the first dose.
  Empty when `ADTM` or `FANLDTM` is empty.
- `AFRLT`: hours from `FANLDTM` to `ADTM`, not rounded; negative before the
  first dose; empty when either is empty.
- `ARRLT`: hours from `PCRFTDTM` to `ADTM`, not rounded. It starts again at
  each dose while `AFRLT` keeps growing, and is negative only before the
  first dose; empty when either is empty.

**Note:** a dose of another drug is never a first or reference dose, and
date/times are clock readings taken as written, with no time zone.
