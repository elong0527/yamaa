# Keep Each Adverse Event's Medication Pairs in Stored Order

**Goal:** give one row for each adverse event and each medication of the same
subject given for it, with their sequence numbers (`AESEQ`, `CMSEQ`), the
event's term (`AETERM`) and the medication's name (`CMTRT`), in the order the
two files store them.

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and reported term, and concomitant medications, each with its sequence
number, name and indication, the indication written as the term of the event
it was given for. Neither file is sorted by subject, term or sequence number.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMTRT`: the name of a medication of the same subject whose indication is
  the event's term.

**Note:** the rows follow the adverse events in the order the events file
stores them, and each event's medications in the order the medications file
stores them, whatever their subjects, terms or sequence numbers. Sorting the
rows by subject and term, by sequence number, or by medication first gives
another order.
