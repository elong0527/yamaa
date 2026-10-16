# Set below-limit concentrations by their place in the profile

**Goal:** give each pharmacokinetic (PK) concentration its analysis value
(`AVAL`), setting a result below the lower limit of quantification by where
it falls in its concentration-time profile.

**Input:** Study Data Tabulation Model (SDTM) PK concentrations (PC), one
record per sample and analyte. A result below the limit is reported as text
starting with `<` (`PCSTRESC`) with no number (`PCSTRESN`); a sample not
done has `PCSTAT` "NOT DONE" and no result. Each sample has its planned
time point (`PCTPT`, numbered in hours in `PCTPTNUM`) and the dose it
follows (`PCTPTREF`).

**Variables:**

- `PCSEQ`, `PARAMCD`: the sample's sequence number and its analyte
  (`PCTESTCD`), as in the input.
- `ATPTREF`, `ATPT`, `ATPTN`: the dose and the planned time point, as in
  the input.
- `AVAL`: the concentration in ng/mL. A quantified result is carried as
  recorded. A result below the limit is 0 when it comes before the first
  quantified result of its profile or after the last one, and has no value
  between them. In a profile with no quantified result, every result below
  the limit is 0. A sample not done has no value.
- `AVALU`: the unit, ng/mL.

**Note:** a profile is the samples of one subject and analyte after one
dose, in planned time order; a sample not done neither starts nor ends the
quantified span. The positions and their values are an authored study
convention, not a recovery of the unmeasured concentration.
