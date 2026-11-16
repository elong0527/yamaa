# Flag below-limit concentrations by their place in the profile

**Goal:** give each pharmacokinetic (PK) concentration its analysis value
(`AVAL`), and flag for exclusion from the noncompartmental analysis
(`NCAXFL`) a result below the lower limit of quantification that falls
between quantified results of its concentration-time profile.

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
  recorded, and every result below the limit is 0. A sample not done has
  no value.
- `AVALU`: the unit, ng/mL.
- `NCAXFL`: `Y` for a result below the limit that comes after the first
  quantified result of its profile and before the last one, so the
  noncompartmental analysis leaves it out; empty otherwise, including a
  result below the limit before the first or after the last quantified
  result, and every result of a profile with no quantified result.

**Note:** a profile is the samples of one subject and analyte after one
dose, in planned time order; a sample not done neither starts nor ends the
quantified span. The positions and their flag are an authored study
convention, not a recovery of the unmeasured concentration.
