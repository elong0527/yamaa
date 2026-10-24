# Pair Each Adverse Event with the Medications Started on or After It

**Goal:** give one row for each adverse event and each medication of the same
subject started on or after the day the event started: their sequence
numbers (`AESEQ`, `CMSEQ`), the event's term (`AETERM`) and the medication's
name (`CMTRT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence
number, reported term and start date, and concomitant medications, each with
its sequence number, name and start date.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMTRT`: the name of a medication of the same subject started on the day
  the event started or later.

**Note:** a medication started before an event, or with no recorded start,
pairs with none of the subject's events, and an event with no medication
started on or after it has no row.
