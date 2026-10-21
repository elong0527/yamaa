# Reject a subject-level variable the subject-level dataset does not carry

**Goal:** give every adverse event record its subject's age (`AGE`), sex
(`SEX`) and baseline weight (`WEIGHTBL`) from the subject-level analysis
dataset (ADSL).

**Input:** adverse event records carrying the reported term (`AETERM`), and
the ADSL that the study's subject-level specification builds from collected
demographics, which records age and sex but no weight.

**Variables:**

- `AGE` and `SEX` would be the subject's values in ADSL, repeated on each of
  their events.
- `WEIGHTBL` would be the subject's baseline weight, which ADSL does not
  record.

**Note:** a variable taken from ADSL keeps the name, type and label ADSL
gives it, so it must be one ADSL publishes. The run is refused before any
record is read instead of inventing a variable ADSL does not have.

## How to fix

Decide where baseline weight comes from. When the subject-level dataset
should carry it, add `WEIGHTBL` to ADSL first; the copy entry can then list
it. Otherwise leave it out of the copy entry and of `output.columns`, or
derive it in this dataset from a source that records it:

```yaml
  - copy:
      dataset: ADSL
      columns: [AGE, SEX]
```
