# Reject Duplicate SITEID

**Goal:** build the analysis dataset with one record per subject
carrying `SITEID`.

**Input:** collected demographics records with `SITEID` giving the
enrolling site.

**Variables:**

- `SITEID` would hold the site the subject enrolled at, copied from
  the collected site.

**Note:** the completed dataset is checked for one record per
site, but a site that enrolled more than one subject repeats its
`SITEID`, which then cannot identify a subject record. The run is
rejected and no artifact is accepted.

## How to fix

Decide whether a record represents a subject or a site before editing
anything. When a record is a subject, the keys already make each subject
appear once; remove the check on `SITEID` rather than restate them:

```yaml
keys: [STUDYID, USUBJID]
```

When one record per site is the intent, aggregate the subjects to site
records before asserting that each site appears once.
