# Reject a Choice of One Medication Beside the Pairing

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

**Note:** every medication of a relationship is to pair with each of its
events, yet the earlier choice of the relationship's first medication is
still stated beside the pairing. The two cannot both hold, so the
specification is rejected before any record is read and no artifact is
accepted.

## How to fix

Decide whether an event takes every medication of its relationships or one
of them. For one row for each event and medication, remove the order and the
choice of the first record:

```yaml
  - id: PAIRS
    dataset: AEMEM
    inner_join: CMMEM
    key: [USUBJID, RELID]
    derivations:
      USUBJID: {type: str, derivation: AEMEM.USUBJID}
      RELID: {type: str, derivation: AEMEM.RELID}
      AESEQ: {type: int, derivation: AEMEM.SEQ}
      CMSEQ: {type: int, derivation: CMMEM.SEQ}
```

For one row for each event with one chosen medication, drive the rows by the
event members and read the medication through an intermediate over the
medication members that declares its own `key`, `order_by` and `keep`, with
no `inner_join`.
