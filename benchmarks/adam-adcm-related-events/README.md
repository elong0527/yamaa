# List the adverse events each medication was given for

**Goal:** carry on each concomitant medication record, named by `CMTRT` and
identified by its sequence number `CMSEQ`, the adverse events it was given
for: their sequence numbers (`AESEQS`), their distinct reported terms
(`AETERMS`) and how many distinct terms there are (`AETERMN`).

**Input:** collected concomitant medications (CM), adverse events (AE) and
related records (RELREC), which tie a medication to the adverse events it was
given for through a shared relationship identifier.

**Variables:**

- In RELAE, one record per adverse event a relationship names: `RELID` is the
  relationship, and `AETERM` the event's reported term, read from the
  subject's adverse event record with that sequence number (`AESEQ`).
- `AESEQS` lists the sequence numbers of the medication's related adverse
  events in increasing numeric order, separated by a comma and a space; blank
  when the medication is related to no adverse event.
- `AETERMS` lists the reported terms of those events in alphabetical order,
  each term once, separated by a comma and a space; blank when there is none.
- `AETERMN` is the number of different terms `AETERMS` lists; blank, not
  zero, when the medication is related to no adverse event.

**Note:** a relationship may also tie the medication to records other than
adverse events, such as medical history, and no list includes those.
Sequence numbers sort as numbers, so 2 comes before 10.
