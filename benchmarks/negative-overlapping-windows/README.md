# Reject Overlapping Windows

**Goal:** give each analysis record the visit (`AVISIT`) of the one
window whose first and last analysis days surround the record's
analysis day (`ADY`).

**Input:** analysis records carrying the analysis day (`ADY`), and
one study-wide window table giving each visit its first (`AWLO`)
and last (`AWHI`) analysis day.

**Variables:**

- `ADY`: the record's analysis day, carried through as given.
- `AVISIT` would be the visit of the window whose first and last
  days, both included, surround the analysis day; missing when no
  window holds the day or the day is missing.

**Note:** a day that falls in two windows has no single visit, and
choosing either would depend on a rule the study did not state, so
the whole run fails and no dataset is produced.

## How to fix

First correct unintended overlap in the window table, so that each
analysis day falls in at most one window (here, end `WEEK 2` on day
14). If overlap is intentional, state and justify a deterministic
selection policy with `order_by` and `keep` on the window lookup; do
not rely on source-row order. For example, to give a shared day to
the window that starts later:

```yaml
intermediates:
  - id: AWIN
    dataset: AWINDOW
    key: STUDYID
    between: {value: ADY, lower: AWLO, upper: AWHI}
    order_by: [AWINDOW.AWLO]
    keep: last
```
