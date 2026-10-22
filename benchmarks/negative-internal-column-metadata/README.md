# Reject Published-Variable Metadata on a Working Value

**Goal:** derive each subject's body mass index at baseline (`BMIBL`) from
the baseline weight (`WEIGHTBL`) and height (`HEIGHTBL`), with the metadata
a data definition describes them by.

**Input:** baseline vital signs, one record per subject, with the weight in
kilograms and the height in centimetres.

**Variables:**

- `WEIGHTBL` and `HEIGHTBL` would be the baseline weight and height as
  collected.
- `BMIBL` would be the weight divided by the square of the height in
  metres.

**Note:** the height in metres is a working value: it is computed on the way
to the body mass index and is not published. It nonetheless carries the
metadata of a published variable, which a data definition could only
describe as a variable the dataset does not contain, so the run is rejected
before any data is read; no artifact is accepted.

## How to fix

Remove the submission metadata from the working value, which is not part of
the dataset a reviewer receives:

```yaml
- name: HEIGHTM
  type: float
  label: Height (m) at Baseline
  derivation: "HEIGHTBL / 100"
```

If the height in metres should be published, list it among the published
columns instead and keep its metadata.
