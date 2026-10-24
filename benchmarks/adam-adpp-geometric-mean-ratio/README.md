# Geometric mean metabolite ratio and replicate variability per subject

**Goal:** derive, for each subject of a three-period study, the geometric
mean of the metabolite-to-parent ratio of maximum concentration across
periods (`PARAMCD` `MRCMAXGM`), and the standard deviation of the replicate
assays of the subject's 2-hour concentration sample (`C2HREPSD`).

**Input:** the SDTM pharmacokinetic (PK) parameters (PP), one metabolite
ratio of maximum concentration (`MRCMAX`) per subject and period, with no
result when the ratio was not calculated; and the SDTM PK concentrations
(PC), one record per assay of a sample, the replicate assays of one sample
sharing its planned time point (`PCTPT`) and numbered by `PCREPNUM`.

**Variables:**

- `PARAMCD`, `PARAM`: `MRCMAXGM` "Geometric Mean Metabolite Ratio Cmax"
  and `C2HREPSD` "Replicate SD of 2 H Concentration".
- `AVAL`: for `MRCMAXGM`, the geometric mean of the subject's ratios above
  zero: the exponential of the mean of their natural logarithms. A ratio of
  zero, from a metabolite concentration below the limit of quantification,
  has no logarithm and is left out, as is a ratio that was not calculated.
  For `C2HREPSD`, the sample standard deviation, with divisor one less than
  the number of assays, of the subject's 2-hour concentrations; it has no
  value for a sample assayed once. Neither is rounded.
- `AVALU`: `RATIO` for `MRCMAXGM` and `ng/mL` for `C2HREPSD`.
