# Reject Pairing Adverse Events with the Rows Being Built

**Goal:** give one row for each adverse event and medication that the
related-records dataset relates to each other: the event's sequence number
(`AESEQ`), the medication's (`CMSEQ`) and the relationship they share
(`RELID`).

**Input:** the related-records dataset (RELREC), one record for each member
of a relationship: the subject, the domain and sequence number of the related
record and the relationship's identifier.

**Variables:**

- `AESEQ` would be the sequence number of an adverse event that is a member
  of the relationship.
- `CMSEQ` would be the sequence number of a medication of the same subject's
  same relationship.
- `RELID` would be the identifier of the relationship they share.

**Note:** the events are to pair with the dataset's own completed rows, but
the pairs are built before any row, from declared records only, so there is
no completed row to pair with. The specification is rejected before any
record is read and no artifact is accepted.

## How to fix

Pair the events with declared records: the medication members of the
related-records dataset, read through an intermediate over it.

```yaml
  - id: CMMEM
    dataset: RELREC
    filter: "RELREC.RDOMAIN = 'CM' AND RELREC.IDVAR = 'CMSEQ'"
    derivations:
      SEQ: {type: int, derivation: IDVARVAL}
  - id: PAIRS
    dataset: AEMEM
    inner_join: CMMEM
    key: [USUBJID, RELID]
```
