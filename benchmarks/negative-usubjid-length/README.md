# Reject Long Identifier

**Goal:** build one demographics record per subject, carrying the
unique subject identifier (`USUBJID`) and the site identifier
(`SITEID`).

**Input:** collected demographics records, each carrying the study
identifier, the site identifier (`SITEID`) and the subject number
(`SUBJID`).

**Variables:**

- `USUBJID` would be the study, site and subject identifiers joined
  by hyphens, such as `CATH-UCSD-0001`; a record missing any of the
  three parts stops the run.
- `SITEID` would be the study site identifier, as collected.

**Note:** the subject identifier must fit in 20 characters on every
record. A site identifier long enough to push it past that limit
rejects the run after the dataset completes, so no artifact is
accepted.

## How to fix

Decide which is authoritative: the width the study declares, or the site
identifier the data carries. If the site identifier can be shortened at
source, correct it there, so that every dataset referring to the subject
keeps one value:

```
CATH,0003,STMARY
```

If the long site identifier is correct, raise the declared width to one that
fits the longest value the study can produce, and change it wherever the
subject identifier is described:

```yaml
- name: USUBJID
  type: str
  derivation:
    str_template: "{DM_RAW.STUDYID}-{DM_RAW.SITEID}-{DM_RAW.SUBJID}"
  verifications:
    - max_length:
        max: 30
```

Do not lower the bound below the values the study produces merely to make
this input pass. A subject identifier that no longer fits its stated width is
a defect wherever it is carried.
