# Reject a Uniqueness Check Over Events Computed Per Subject

**Goal:** carry each subject's disposition event (`EOSSTT`) and the days
from the last dose to it (`EOSPDY`) onto the subject-level analysis dataset.

**Input:** collected demographics with the study day of the last dose
(`TRTEDY`), one record per subject, and collected disposition events with
their category and study day (`DSSTDY`).

**Variables:**

- `TRTEDY` would be the study day of the last dose as collected.
- `EOSSTT` would be the subject's disposition event.
- `EOSPDY` would be the study day of that event minus the study day of the
  last dose.

**Note:** the disposition events are checked to be one per subject before
any record is built, but each event also carries a value computed from the
record being built, its last dose day. A check made once cannot cover events
whose values change record by record, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Keep the uniqueness check on the event itself, and compute the days from the
last dose in the subject-level column instead:

```yaml
- name: EOSPDY
  type: int
  label: Days From Last Dose to End of Study
  derivation: "DISP.DSSTDY - TRTEDY"
```

with `DAYSPOST` removed from the event's `derivations`.
