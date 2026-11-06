# Reject an Event With Two Qualifier Records of One Name

**Goal:** carry the location detail (`AEXSITE`) and the suspected cause
(`AEXCAUS`) of each adverse event onto its analysis row.

**Input:** adverse event records and the supplemental qualifier records of
the AE domain, each naming the parent event by its sequence number, the
qualifier name and its value.

**Variables:**

- `AEXSITE` would contain the value of the event's one `AEXSITE` qualifier.
- `AEXCAUS` would contain the value of the event's one `AEXCAUS` qualifier.

**Note:** each qualifier name is answered by one record of that name, and
nothing says which of two to take, so an event with two `AEXSITE` records
has no single answer, and the run is rejected with no artifact accepted. The
other qualifier name of the same event takes no part in the rejection.

## How to fix

Decide which record answers. When the later record of a name supersedes the
earlier, keep the last in stored order by a field that orders them, such as
a sequence number read as an integer:

```yaml
input:
  SUPPAE: {path: input/suppae.csv, types: {IDVARVAL: int, QSEQ: int}}

intermediates:
  - id: QUAL
    dataset: SUPPAE
    key: {STUDYID: AE.STUDYID, USUBJID: AE.USUBJID, IDVARVAL: AE.AESEQ}
    filter: "SUPPAE.RDOMAIN = 'AE' AND SUPPAE.IDVAR = 'AESEQ'"
    order_by: [SUPPAE.QSEQ]
    keep: last
    pivot: {name: QNAM, value: QVAL, fields: [AEXSITE, AEXCAUS]}
```

When a repeated qualifier is a data error, leave the declaration as it is
and correct the supplemental records.
