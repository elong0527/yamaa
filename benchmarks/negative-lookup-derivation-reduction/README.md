# Refuse a Reduction Inside a Donor Selection

Goal: show that a donor selection cannot reduce a whole study inside the
distance it computes for each candidate donor.

Input: `spec.yaml` reads one subjects file: study, subject, baseline age,
baseline score and an observed lesion count, missing for some subjects. It
would copy a missing count from the subject of the same study whose age lies
nearest, scaling the age difference by the study standard deviation.

Note: the distance is computed once for each candidate donor, from that
donor's record and the subject being imputed, so a standard deviation over
the whole study has no records to reduce there. The run stops before any
row is built, and the finding names the place such a value is read from.

## How to fix

Reduce the study in a column, and read that column in the distance through
the output domain's name:

```yaml
columns:
  - name: SDAGE
    type: float
    derivation: {aggregate: {expr: "STDDEV(AGE)", group_by: [STUDYID]}}
intermediates:
  - id: DONOR
    derivations:
      DIST: "ABS(AGE - LESION.AGE) / LESION.SDAGE"
```
