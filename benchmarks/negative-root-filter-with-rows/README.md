# Reject a Record Selection Stated Twice

**Goal:** keep one record per serious adverse event, carrying its
dictionary-derived term (`AEDECOD`).

**Input:** collected adverse events, one record per event, with the
seriousness flag (`AESER`).

**Variables:**

- `AESEQ` would be the collected sequence number of the event.
- `AEDECOD` would be the dictionary-derived term as collected.

**Note:** which events become records is stated in two places at once: a
selection of serious events for the whole dataset, and a separate template
that builds one record from every collected event. The two could disagree
about which events appear, so the run is rejected before any data is read
and no artifact is accepted.

## How to fix

Decide which events the dataset holds, then state that selection once. To
keep only serious events, move the condition into the row template and drop
the root `filter`:

```yaml
rows:
  - id: event
    dataset: AE
    filter: "AE.AESER = 'Y'"
```

Without any row template, the root `filter` alone selects the records.
