# Reject an Analysis Age Described as Collected From the Subject

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, with the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years, copied unchanged from demographics.

**Note:** the age is described as collected from the subject. An analysis
dataset collects nothing: its values come from earlier datasets or are
computed or assigned, so a collected origin names a source the analysis
model does not admit, and the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Describe where this dataset took the value from. Here it is copied from
demographics, which recorded it as collected:

```yaml
origins:
  - type: Predecessor
    description: DM.AGE
```
