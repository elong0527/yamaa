# Reject a Randomization Matched on One Administration's Site

**Goal:** build one record per subject from the subject's exposure records,
with the subject's planned arm in `ARM` and the total dose administered in
`TOTDOSE`.

**Input:** exposure records, one per administration, carrying the site
(`SITEID`) and the dose given (`EXDOSE`), and a randomization list giving each
subject's planned arm (`ARM`) by site and subject.

**Variables:**

- `ARM` would be the subject's planned arm from the randomization list, and
  blank when the list has no record for the subject at that site.
- `TOTDOSE` would be the sum of the doses of the subject's administrations.

**Note:** each subject's record summarizes all of the subject's
administrations, and the randomization record is matched on an
administration's site. The subject's record holds only the study and the
subject, so it has no site to match on, and the run is rejected before any
data is read and no artifact is accepted.

## How to fix

A subject is enrolled at one site, so add the site to the record's grouping.
It does not split any subject's record, and the record then holds the site to
match on:

```yaml
rows:
  - id: subject
    dataset: EX
    group_by: [EX.STUDYID, EX.SITEID, EX.USUBJID]
```

When the randomization list identifies subjects without their site, match on
the subject alone instead.
