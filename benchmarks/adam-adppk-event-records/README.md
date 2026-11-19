# Dose and Concentration Event Records for Population PK

**Goal:** put a subject's dose administrations and pharmacokinetic (PK)
concentration samples in one dataset, one record per event in time order,
as a population PK model reads them: the event type (`EVID`), the
compartment (`CMT`), the dose amount (`AMT`), the concentration (`DV`),
whether a record has no concentration (`MDV`), a subject number (`ID`) and
the record's place in the subject's sequence (`ASEQ`).

**Input:** exposure (EX) records, one per administration, with the dose
(`EXDOSE`) and its date/time (`EXSTDTC`), and PK concentration (PC)
records, one per sample and analyte, with the numeric result (`PCSTRESN`,
empty when the result is below the limit of quantification), the
completion status (`PCSTAT`) and the collection date/time (`PCDTC`).

**Variables:**

- `ID`: the subject's number among the subjects in the dataset, counted
  from 1 in the order of `USUBJID`, with no number skipped.
- `ASEQ`: the record's number within the subject, from 1 in the order of
  `ADTM`. At one date/time a sample comes before a dose, since it was
  drawn before the dose was given, and samples follow their `CMT`.
- `ADTM`: the date/time of the administration or of the sample.
- `EVID`: 1 for a dose record and 0 for a concentration record.
- `CMT`: 1 for a dose, the compartment the drug is given into; 2 for a
  concentration of the parent drug (`DRGX`) and 3 for one of its
  metabolite (`MET1`).
- `PARAMCD`: `DOSE` for a dose record and the analyte for a concentration
  record.
- `AMT`: the dose given; empty on a concentration record.
- `DV`: the concentration; empty on a dose record and for a result below
  the limit of quantification.
- `MDV`: 1 when `DV` is empty and 0 otherwise.

**Note:** an administration with a dose of zero is no event and gives no
record, a sample that was not done gives none, and a subject with no
administration of an amount, such as a placebo subject, has no records at
all.
