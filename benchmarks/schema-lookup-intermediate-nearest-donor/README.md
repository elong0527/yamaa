# Impute a Missing Outcome from the Nearest Donor

Goal: derive `AOUTCOME`, the lesion count each subject is analysed with,
copying a missing count from the most similar subject of the same study,
and record that donor in `DONORID` and its distance in `DONORDST`.

Input: `spec.yaml` reads one subjects file: study, subject, baseline age,
baseline score and the observed lesion count, missing for some subjects.

Variables:

- `AGE`, `BSCORE` and `OUTCOME` are the subject's baseline age, baseline
  score and observed lesion count, as collected.
- `SDAGE` and `SDBSCORE` are the sample standard deviations of age and of
  baseline score over every subject of the study with a value.
- `DONORID` is the donor of a subject whose count is missing: the subject of
  the same study with an observed count whose age and score lie nearest, each
  difference divided by its study standard deviation. It is missing for a
  subject with an observed count, and when the study has no donor.
- `DONORDST` is that donor's distance, the square root of the sum of the two
  squared scaled differences.
- `AOUTCOME` is the observed count, or else the donor's count; it is missing
  when the subject has neither.

Note: subject 104 of study S1 is nearest subject 103 (age 55 against 54,
score 11 against 12), so it is analysed with 103's count of 3. Subject 105
has no age, so it has no distance and is never a donor. In study S2, subjects
201 and 202 are equally near 203; the lower subject number, 201, is the
donor. Study S3 has no subject with an observed count, so its subjects keep a
missing count. The standard deviations are columns of the dataset, which the
donor selection reads for the subject being imputed.
