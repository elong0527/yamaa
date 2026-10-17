# Reject a Subject Without a Planned Treatment

**Goal:** carry each subject's planned treatment (`TRT01P`) from the
demographics arm, required for every subject.

**Input:** collected demographics with the study and subject
identifiers and the planned arm.

**Variables:**

- `TRT01P` would be the subject's planned treatment, copied from the
  demographics arm.

**Note:** the data definition states that every subject carries a
planned treatment, and that statement is checked against the data. A
subject whose arm was never recorded rejects the run once the planned
treatments are complete, so no artifact is accepted.

## How to fix

Lead with the clinical question: a subject in the analysis population
was randomized to an arm, so a missing arm is a data issue. Query the
site, correct the arm at its source and rerun.

When the analysis genuinely admits subjects without a planned treatment,
such as screen failures kept in the dataset, state that in the submission
metadata instead:

```yaml
submission:
  core: Req
  mandatory: false
  origin:
    type: Predecessor
    description: DM.ARM
```

Do not add a separate not-missing check beside `mandatory: true`; the
metadata already enforces it.
