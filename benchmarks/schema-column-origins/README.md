# Describe a Result Reported by Two Laboratories

**Goal:** submit the laboratory results (`LBTESTCD`, `LBTEST`, `LBORRES`)
with a data-definition document that states both places a result can come
from.

**Input:** `spec.yaml` reads the laboratory results, one row per test per
subject, and `project.yaml` submits the dataset with the study's annotated
case report form.

**Variables:**

- `LBTESTCD` and `LBTEST` are the test's code and name: `HGB`, hemoglobin.
- `LBORRES` is the result as reported, in the reporting laboratory's units.

**Note:** the central laboratory reports most results, but a site whose
sample could not be shipped tests it in its own laboratory and records the
result on page 12 of the case report form. No other variable tells the two
apart, so the result's definition names both: transmitted by the central
laboratory, and collected by the investigator on that page, in that order.
