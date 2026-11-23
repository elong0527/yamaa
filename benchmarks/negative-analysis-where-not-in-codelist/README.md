# Refuse an Analysis Result That Selects a Code the Study Does Not List

**Goal:** carry `PARAMCD`, `PARAM`, `AVAL` and `AVALC` into a laboratory
analysis dataset, and describe the analysis behind the glucose table in the
data-definition document.

**Input:** standardized laboratory results (SDTM LB), one record per subject
and test, with the result as text, as a number when it is one, and its unit.

**Variables:**

- `PARAMCD` is the laboratory test code, one of `GLUC`, `CREAT` and `HCG`.
- `PARAM` names the parameter, with its unit for the numeric tests.
- `AVAL` is the numeric result; missing for the pregnancy test.
- `AVALC` is the result as text.

**Note:** the glucose table says it summarises the records whose parameter
code is `GLUCOSE`, a code the study's list of parameter codes does not hold:
glucose is `GLUC`. The table would describe records that cannot exist, so the
submission is refused before any document is written; the laboratory dataset
itself still builds.

## How to fix

Select the records by the code the dataset carries:

```yaml
datasets:
  ADLB:
    where:
      PARAMCD: [GLUC]
    variables: [AVAL]
```

When `GLUCOSE` is a parameter the study analyses, add it to the list of
parameter codes and derive its records, rather than selecting a code no
record holds.
