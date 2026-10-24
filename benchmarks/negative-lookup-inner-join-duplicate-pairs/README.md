# Reject Two Treating Medications Where Each Adverse Event Has One

**Goal:** give one row for each adverse event with the one medication of the
same subject given for it: its sequence number (`CMSEQ`) and name (`CMTRT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and concomitant medications, each with its sequence
number, name and indication, the indication written as the term of the event
it was given for.

**Variables:**

- `CMSEQ` would be the sequence number of the medication given for the event.
- `CMTRT` would be the name of that medication.

**Note:** each event is to have one treating medication, yet both subjects'
headaches were treated with two medications each, so their events and
medications pair twice. Nothing says which medication an event keeps, so the
run stops before any row is built and no artifact is accepted.

## How to fix

Confirm with the data owner whether an event may have several treating
medications. If it may, keep every pair and give each its own row, adding the
medication's sequence number to the row identifiers and removing the
one-medication expectation:

```yaml
keys: [USUBJID, AESEQ, CMSEQ]
```

If an event keeps one of them, state which one: read the pairs of each event
through an intermediate that declares a key of the subject and the event's
sequence number, an order and the record to keep, such as the medication
with the lowest sequence number.
