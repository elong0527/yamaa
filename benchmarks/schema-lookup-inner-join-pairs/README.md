# Pair Each Adverse Event with Each Medication of Its Relationship

**Goal:** give one row for each adverse event and medication that the
related-records dataset relates to each other: the event's sequence number
(`AESEQ`), the medication's (`CMSEQ`) and the relationship they share
(`RELID`).

**Input:** one `spec.yaml` over the related-records dataset (RELREC), one
record for each member of a relationship: the subject, the domain and
sequence number of the related record and the relationship's identifier.

**Variables:**

- `AESEQ`: the sequence number of an adverse event that is a member of the
  relationship.
- `CMSEQ`: the sequence number of a medication that is a member of the same
  subject's same relationship.
- `RELID`: the identifier of the relationship the event and the medication
  share.

**Note:** each event of a relationship pairs with each medication of the same
subject's same relationship, so a relationship gives as many rows as its
events times its medications. A medication that belongs to two relationships
pairs with the events of each, and two subjects that reuse one relationship
identifier never pair with each other. The rows follow the events in their
stored order, and each event's medications in theirs.
