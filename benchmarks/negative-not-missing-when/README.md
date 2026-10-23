# Reject a Reported Event Without Its Coded Term

**Goal:** carry each adverse event's reported term (`AETERM`) and coded term
(`AEDECOD`), requiring a coded term on every event with a reported term.

**Input:** collected adverse events, one record per subject and event, with
the term as reported and its dictionary code.

**Variables:**

- `AETERM` would be the event's term as the investigator reported it,
  missing where none was reported.
- `AEDECOD` would be the dictionary-derived term for the reported term,
  required wherever a term was reported; a record without a reported term
  may lack it.

**Note:** one event reports a term without its coded term, so the check on
the completed dataset fails and no artifact is accepted. The record that
reports no term lacks a coded term too, which the rule allows.

## How to fix

Code the reported term at the source, where the dictionary coding is
governed, and rerun. A reported term that has not been coded yet is not a
reason to relax the rule for every record; when coding is still under way
and the dataset must build, report the uncoded events instead of failing,
which needs a run report file:

```yaml
output:
  report: ae-checks.csv
columns:
  - name: AEDECOD
    verifications:
      - not_missing: {when: "AETERM IS NOT NULL", severity: warning}
```

Do not copy the reported term into the coded term, which claims a coding
nobody did.
