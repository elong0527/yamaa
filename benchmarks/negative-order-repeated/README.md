# Reject Repeated Order

**Goal:** arrange adverse event (AE) records so each subject's
events read together, newest onset first, carrying the reported
term (`AETERM`) and onset date (`ASTDT`).

**Input:** collected adverse event records carrying the reported
term (`AETERM`) and the onset date (`AESTDTC`).

**Variables:**

- `AETERM` would be the reported term for the event, carried over
  from the collected records.
- `ASTDT` would be the date the event began, carried over from the
  collected onset date.

**Note:** the records are to be presented by subject ascending,
then by onset date with the most recent first, then by subject
descending again. The subject takes two places with opposite
directions, and the second can never change the order the first
already fixed, so the intended order is unclear. The run is
rejected before any data is read and no artifact is accepted.

## How to fix

Decide which position the subject holds and state it once. Grouping a
subject's events together while reading the newest first is the subject
ascending first and the date descending:

```yaml
output:
  path: adae.csv
  columns: [STUDYID, USUBJID, ASEQ, AETERM, ASTDT]
  order_by:
    - USUBJID
    - variable: ASTDT
      direction: desc
```

Presenting the subjects in reverse instead means declaring the subject
descending once, in the first position, rather than adding a second entry for
it.
