# Reject a Baseline Body Mass Index From an Unchosen Screening Weight

**Goal:** give each subject a baseline body mass index in `BMIBL`, computed
by the project's own routine from the subject's first height measurement and
screening weight.

**Input:** demographics records, one per subject; height measurements
carrying the study day (`VSDY`) and the height (`HEIGHTCM`); and weight
records carrying the visit (`VISIT`), the order recorded (`WTSEQ`) and the
weight (`WEIGHTKG`), where a subject may have been weighed more than once at
screening.

**Variables:**

- `BMIBL` would be the body mass index of the subject's first height
  measurement with the screening weight, and blank when either is missing.

**Note:** the routine takes one weight for each height measurement, but a
subject weighed twice at screening has two screening weights, and nothing
states which one to use. The run is rejected before any data is read and no
artifact is accepted.

## How to fix

Decide which screening weight is the baseline, and state it on the screening
weights, for example the last one recorded:

```yaml
- id: SCREENWT
  dataset: WEIGHT
  key: [USUBJID]
  filter: "WEIGHT.VISIT = 'SCREENING'"
  order_by: [WEIGHT.WTSEQ]
  keep: last
```
