# Concentrations Below the Limit of Quantification Under Two Methods

**Goal:** prepare the pharmacokinetic (PK) concentrations of a population
PK dataset that are below the limit of quantification for two ways of
handling them: as censored at the limit (the M3 method: `CENS`, `DV`,
`MDV`), and as half the limit for the first one after a quantified result
(the M6 method: `DVM6`, `MDVM6`), with the limit itself (`ALLOQ`).

**Input:** exposure (EX) records, one per administration, with the dose
(`EXDOSE`) and its date/time (`EXSTDTC`), and PK concentration (PC)
records of one analyte, one per sample, with the collection date/time
(`PCDTC`), the limit of quantification (`PCLLOQ`) and the result. A
quantified result has its number in `PCSTRESN`; a result below the limit
has none and is reported as text starting with `<` (`PCSTRESC`). An
administration gives a dose record, and a sample gives a concentration
record unless it was not done (`PCSTAT`).

**Variables:**

- `ASEQ`: the record's number within the subject, in the order of `ADTM`,
  a sample before a dose at the same date/time.
- `ADTM`: the date/time of the administration or of the sample.
- `EVID`: 1 for a dose record and 0 for a concentration record.
- `AMT`: the dose given; empty on a concentration record.
- `ALLOQ`: the limit of quantification of the sample; empty on a dose
  record.
- `CENS`: 1 for a result below the limit; 0 for a quantified result and on
  a dose record.
- `DV`: the quantified result, or the limit for a result below it; empty
  on a dose record.
- `MDV`: 1 when `DV` is empty and 0 otherwise.
- `DVM6`: the quantified result; for a result below the limit, half the
  limit when the subject's sample before it was quantified, and empty when
  that sample was also below the limit or when there is no sample before
  it. Empty on a dose record.
- `MDVM6`: 1 when `DVM6` is empty and 0 otherwise.

**Note:** the sample before a result is the subject's latest earlier
concentration record, whatever doses were given in between; a sample that
was not done is no record, so it does not separate a result below the
limit from the quantified result before it.
