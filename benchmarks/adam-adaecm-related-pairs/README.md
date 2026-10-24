# Pair Each Adverse Event with Each Medication Related to It

**Goal:** give one row for each adverse event and each medication that the
related-records dataset relates to it: the event's sequence number and
reported term (`AESEQ`, `AETERM`), the medication's sequence number and name
(`CMSEQ`, `CMTRT`), and the relationship they share (`RELID`).

**Input:** collected adverse events (AE), concomitant medications (CM) and
related records (RELREC), one record for each member of a relationship: the
subject, the domain and sequence number of the related record and the
relationship's identifier.

**Variables:**

- `AETERM`: the reported term of the adverse event.
- `CMTRT`: the reported name of the medication.
- `RELID`: the identifier of the subject's relationship that relates the
  event and the medication.

**Note:** every event of a relationship pairs with every medication of the
same subject's same relationship, so a relationship of two events and two
medications gives four rows, and a medication related to two events through
two relationships appears once with each. Relationship identifiers count
within a subject: two subjects' relationships with the same identifier are
different relationships.
