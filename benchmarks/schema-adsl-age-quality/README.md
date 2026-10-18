# Keep Out-of-Range Ages for Review

**Goal:** carry each subject's collected age (`AGE`) into the ADSL analysis
dataset, keeping values outside the expected range of 18 to 100 for later
data review.

**Input:** collected demographics, one record per subject, carrying age.

**Variables:**

- `AGE`: the subject's age in years as collected. A value below 18 or above
  100 stays in the dataset as collected, and a missing age stays missing;
  neither case stops the run.

**Note:** an age outside 18 to 100 does not stop the run: every subject keeps
its record, and the run report (`adsl-violations.csv`) records the review.
Each of its rows names the dataset (`ARTIFACT`), what kind of item it
describes (`KIND`), where the spec states that item (`SPEC_PATH`) and its
subject (`NAME`): the run and its outcome (`OUTCOME`), the demographics
read, the subjects built, each output column with its missing values and
longest text, and the age check. The check's row gives the column checked
(`TARGET`), the rule it applies (`RULE`), how serious a failure is
(`SEVERITY`), the failure (`CONDITION`), how many subjects failed out of how
many were checked (`COUNT`, `TOTAL`), every subject that failed (`KEYS`)
and the remaining facts (`DETAILS`); the check carries no label of its own
(`ID`).
