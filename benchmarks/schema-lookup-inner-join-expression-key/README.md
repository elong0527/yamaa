# Pair Each Adverse Event with Its Medications, Matching Terms in Upper Case

**Goal:** give one row for each adverse event and each medication of the same
subject given for it: their sequence numbers (`AESEQ`, `CMSEQ`), the event's
term (`AETERM`) and the medication's name (`CMTRT`).

**Input:** one `spec.yaml` over adverse events, each with its sequence number
and its term as reported, in mixed case, and concomitant medications, each
with its sequence number, name and indication, coded in upper case.

**Variables:**

- `AETERM`: the reported term of the adverse event, as reported.
- `CMTRT`: the name of a medication of the same subject whose indication is
  the event's term, compared in upper case.

**Note:** an event pairs with every medication of its subject given for it,
so an event treated twice has two rows, and a medication given for a term no
event of its subject reports, such as a rash, has none.
