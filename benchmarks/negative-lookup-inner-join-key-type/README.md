# Reject a Rescue Medication Named by a Word Where a Number Is Recorded

**Goal:** give one row for each adverse event with the rescue medication its
reported term calls for: the medication's sequence number in the subject's
records (`CMSEQ`) and its name (`CMTRT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and concomitant medications, each with its sequence number
and name; a list gives, for each term, the sequence number of the
medication it calls for.

**Variables:**

- `CMSEQ` would be the sequence number of the medication the event's term
  calls for.
- `CMTRT` would be the name of that medication.

**Note:** the list gives a headache's medication as the number 1, but a
nausea's as the word `two`, which cannot be compared with a recorded
sequence number. The first nausea compared with a medication stops the run,
and no artifact is accepted.

## How to fix

Write every entry of the list as the medication's sequence number:

```yaml
    key:
      USUBJID: AE.USUBJID
      CMSEQ:
        mapping:
          source: AE.AETERM
          dict: {HEADACHE: 1, NAUSEA: 2}
```
