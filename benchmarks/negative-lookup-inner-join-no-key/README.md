# Reject Pairing Events with Medications Without Saying What They Share

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

**Note:** the events are to pair with the medications, yet nothing says what
an event and a medication must share to pair: every event would pair with
every medication of every subject. The specification is rejected before any
record is read and no artifact is accepted.

## How to fix

Name the fields an event member and a medication member share to pair, here
the subject and the relationship:

```yaml
  - id: PAIRS
    dataset: AEMEM
    inner_join: CMMEM
    key: [USUBJID, RELID]
```

To pair every event with every medication on purpose, name a field both
sides hold with one value, such as the study identifier.
