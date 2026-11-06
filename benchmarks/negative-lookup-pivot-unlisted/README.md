# Reject a Qualifier Record Whose Name Is Not Declared

**Goal:** carry the location detail (`AEXSITE`) and the suspected cause
(`AEXCAUS`) of each adverse event onto its analysis row.

**Input:** adverse event records and the supplemental qualifier records of
the AE domain, each naming the parent event by its sequence number, the
qualifier name and its value; one record carries a reaction size, which
the analysis does not declare.

**Variables:**

- `AEXSITE` would contain the value of the event's `AEXSITE` qualifier.
- `AEXCAUS` would contain the value of the event's `AEXCAUS` qualifier.

**Note:** the declared qualifier names are the complete list of what an
event's qualifier records may carry. A record named otherwise, here the
reaction size of the first event, has no column to go to, so the run is
rejected with no artifact accepted rather than dropping the value silently.

## How to fix

Decide whether the qualifier belongs in the analysis. When it does, list it
and give it a column:

```yaml
intermediates:
  - id: QUAL
    dataset: SUPPAE
    key: {STUDYID: AE.STUDYID, USUBJID: AE.USUBJID, IDVARVAL: AE.AESEQ}
    filter: "SUPPAE.RDOMAIN = 'AE' AND SUPPAE.IDVAR = 'AESEQ'"
    pivot: {name: QNAM, value: QVAL, fields: [AEXSITE, AEXCAUS, AEXSIZE]}

columns:
  - name: AEXSIZE
    type: int
    label: Reaction Size (mm)
    derivation: QUAL.AEXSIZE
```

When it is left out on purpose, narrow the filter to the names the analysis
reads, so the record is never a candidate:

```yaml
    filter: >-
      SUPPAE.RDOMAIN = 'AE' AND SUPPAE.IDVAR = 'AESEQ'
      AND SUPPAE.QNAM IN ('AEXSITE', 'AEXCAUS')
```
