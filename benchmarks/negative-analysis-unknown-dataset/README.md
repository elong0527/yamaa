# Refuse an Analysis Result That Reads a Dataset the Study Does Not Describe

**Goal:** carry `PARAMCD`, `PARAM`, `AVAL` and `AVALC` into a laboratory
analysis dataset, and describe the analysis behind the glucose table in the
data-definition document.

**Input:** standardized laboratory results (SDTM LB), one record per subject
and test, with the result as text, as a number when it is one, and its unit.

**Variables:**

- `PARAMCD` is the laboratory test code: glucose, creatinine and the
  pregnancy test.
- `PARAM` names the parameter, with its unit for the numeric tests.
- `AVAL` is the numeric result; missing for the pregnancy test.
- `AVALC` is the result as text.

**Note:** the glucose table says it takes its subjects from the subject-level
dataset `ADSL`, but the study describes the laboratory dataset alone. The
data-definition document would point a reviewer to a dataset it does not
describe, so the submission is refused before any document is written; the
laboratory dataset itself still builds.

## How to fix

Describe the subject-level dataset in the same study, so the table can read
it:

```yaml
adam:
  specs:
  - adsl.yaml
  - spec.yaml
```

When the table does not use `ADSL`, remove it from the result instead, with
the `join` that only several datasets need:

```yaml
datasets:
  ADLB:
    where:
      PARAMCD: [GLUC]
    variables: [AVAL]
```
