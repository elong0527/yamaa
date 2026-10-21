# Describe Central and Site Laboratory Results Apart

**Goal:** submit the laboratory results (`LBTESTCD`, `LBTEST`, `LBORRES`)
with a data-definition document that describes the results the site tests
apart from all the others.

**Input:** `spec.yaml` reads the laboratory results, one row per test per
subject, and `project.yaml` submits the dataset with the study's annotated
case report form.

**Variables:**

- `LBTESTCD` and `LBTEST` are the test's code and name.
- `LBORRES` is the result as reported, in the reporting laboratory's units.

**Note:** the site tests the pregnancy (`HCG`) and urine pH (`PH`) samples
and records them on page 14 of the case report form; the central laboratory
reports every other test. The document describes `LBORRES` once for the tests
other than those two, as transmitted by the central laboratory, and once for
the two, as collected by the investigator. The two descriptions select no
common row.
