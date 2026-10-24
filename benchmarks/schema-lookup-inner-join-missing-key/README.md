# Pair Each Adverse Event with the Medications Recorded at Its Visit

**Goal:** give one row for each adverse event and medication of the same
subject recorded at the same visit: their sequence numbers (`AESEQ`,
`CMSEQ`) and the visit (`VISIT`).

**Input:** one `spec.yaml` over adverse events and concomitant medications,
each with its subject, its sequence number and the visit it was recorded at.

**Variables:**

- `VISIT`: the visit the event and the medication were both recorded at.

**Note:** a record with no subject or no visit pairs with nothing, not even
with another record whose visit is also empty: an empty value is no visit
that two records can share.
