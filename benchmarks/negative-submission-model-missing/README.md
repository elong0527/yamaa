# Reject Submission Metadata That Names No Model

**Goal:** describe demographics (`STUDYID`, `USUBJID`, `SEX`) for a
submission.

**Input:** one record per subject, with the sex as collected.

**Variables:**

- `SEX` is the sex as collected.

**Note:** metadata that describes a dataset for a submission is judged
against the CDISC model the dataset follows, SDTM, SEND or ADaM, which
decides the core designations, origins and data types it may use. Metadata
that does not say which model it follows is rejected before any data is read,
and no artifact is written.

## How to fix

State the model the dataset follows. Demographics is an SDTM domain:

```yaml
yamaa: "1.0"
type: spec
model: SDTM
```
