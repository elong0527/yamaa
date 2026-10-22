# Reject a Call to a Project Routine the Project Does Not List

**Goal:** derive each subject's baseline body mass index (`BMIBL`) from the
baseline weight (`WEIGHTBL`) and height (`HEIGHTBL`) with the study's `bmi`
routine.

**Input:** baseline vital signs, one record per subject, with the weight in
kilograms and the height in centimetres.

**Variables:**

- `WEIGHTBL` and `HEIGHTBL` would be the baseline weight and height as
  collected.
- `BMIBL` would be the weight divided by the square of the height in
  metres, as the `bmi` routine computes it.

**Note:** the body mass index calls a routine named `bmi`, but the project
lists no routine of that name. The study's routines are only those its
project lists, so the run is rejected before any project code or data is
read, and no artifact is accepted.

## How to fix

Add the study's function file for `bmi` to the project, beside the
specifications it serves:

```yaml
functions:
- functions/bmi.yaml
```

The file names the `bmi` routine, its parameters and tests, and the
callable that implements it in each host language.
