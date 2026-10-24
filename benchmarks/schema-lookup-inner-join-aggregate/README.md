# Count the Medications Related to Each Adverse Event

**Goal:** give each adverse event the number of medications the
related-records dataset relates to it (`CMCOUNT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and the related-records dataset (RELREC), one record for
each member of a relationship.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMCOUNT`: how many medications share a relationship with the event, a
  medication counted once for each relationship it shares with the event;
  empty when no relationship relates the event to a medication.
