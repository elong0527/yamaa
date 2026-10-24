# Find the Last Medication Related to Each Adverse Event

**Goal:** give each adverse event the sequence number of the last medication
the related-records dataset relates to it (`CMSEQ`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and the related-records dataset (RELREC), one record for
each member of a relationship.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMSEQ`: the highest sequence number among the medications that share a
  relationship with the event, whichever of the event's relationships they
  belong to, whatever order the related records list them in; empty when no
  relationship relates the event to a medication.

**Note:** every event and medication of a relationship are first paired; the
pairs of one event are then read like the records of a collected dataset,
keyed by the subject and the event's sequence number, and the one with the
highest medication sequence number is kept.
