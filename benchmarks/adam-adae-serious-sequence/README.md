# Serious Event Sequence

**Goal:** number each subject's serious events (`AESER` is `Y`) in
onset order as `SERSEQ`, so the earliest serious event carries 1.

**Input:** adverse event records carrying the collection sequence
(`AESEQ`), the serious flag (`AESER`), and the onset date
(`AESTDTC`, carried as `ASTDT`; empty when the onset was never
collected).

**Variables:**

- `SERSEQ` holds the running number of the subject's serious events,
  ordered by onset date. Events sharing one onset date follow the
  order of their collection sequence, and a serious event with no
  onset date is numbered last. It stays empty for events that are
  not serious.

**Note:** numbering restarts for each subject, so a subject with no
serious event keeps every record unnumbered.
