# Reject an Adverse Event Whose Subject Has No Subject-Level Record

**Goal:** derive each adverse event's analysis start date (`ASTDT`), the
subject's date of first exposure to treatment (`TRTSDT`) and the
treatment-emergent flag (`TRTEMFL`).

**Input:** adverse events carrying each event's start date, and
subject-level records carrying each subject's first exposure date.

**Variables:**

- `ASTDT` would be the event's start date.
- `TRTSDT` would be the first exposure date of the event's subject.
- `TRTEMFL` would be `Y` when the event starts on or after the first
  exposure date.

**Note:** every adverse event is required to belong to a subject with a
subject-level record, so an event whose subject has none rejects the run with
no artifact accepted, instead of leaving its first exposure date and its
treatment-emergent flag blank.

## How to fix

Add the subject-level record of subject `YAMAA-01-104` when the subject belongs
to the analysis population, or remove the event when the subject does not
belong to the study. If events of subjects outside the subject-level data are
acceptable, drop `require_match: true` from the input:

```yaml
input:
  ADSL: {path: input/adsl.csv, types: {TRTSDT: date}}
```

The first exposure date is then missing for such an event, its flag is blank,
and the run counts each such event under `absent` at the date's derivation.
