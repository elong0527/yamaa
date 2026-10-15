# Reject an undefined visit ordering

**Goal:** compute `VISITSEQ`, the sequence number of each subject's
vital-sign visits in analysis visit order.

**Input:** vital-sign visit records identified by subject (`USUBJID`) and
analysis visit number (`AVISITN`).

**Variables:**

- `VISITSEQ` would number the subject's visits from one in analysis visit
  order. It names that ordering with a misspelling that matches no defined
  ordering, so the run is rejected before any data is read and no artifact
  is accepted.

## How to fix

Correct the misspelled reference to the complete declared window:

```yaml
row_number:
  window: VISIT_ORDER
```

If a different ordering is intended, declare it as its own complete window
or write its settings inline.
