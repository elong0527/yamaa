# Reject a Dose per Kilogram From a Zero Weight

**Goal:** derive each subject's first dose per kilogram of body weight
(`DOSEKG`) with the study's own calculation.

**Input:** one record per subject with the first dose in milligrams
(`DOSEMG`) and the baseline weight in kilograms (`WEIGHTKG`).

**Variables:**

- `DOSEMG` and `WEIGHTKG` are the dose and the weight as collected.
- `DOSEKG` would be the dose divided by the weight, from the study's
  `dose_per_kg` calculation.

One subject's weight was recorded as 0 kg. The study's calculation refuses
a weight that is not positive and stops with an error, rather than return a
dose per kilogram that means nothing. Its error stops the run, and no
artifact is accepted.

## How to fix

Find the subject's baseline weight in the source. A weight of zero is a
recording error, most often a measurement that was never entered; correct it
and export the record again:

    STUDYID,USUBJID,DOSEMG,WEIGHTKG
    CTX,CTX-81,150,60.0
    CTX,CTX-82,200,74.0
    CTX,CTX-83,100,50.0

If the study decides that an impossible weight gives no dose per kilogram,
change the calculation to return a missing value for it, and declare
`may_return_missing: true` in its function file.
