# Reject a subject identifier that exceeds its length limit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-usubjid-exceeds-length.html)

**Goal:** attempt the subject-level demographics dataset carrying
the site identifier (`SITEID`).

**Input:** collected demographics rows, each carrying the site
identifier (`SITEID`) and the subject number.

**Variables:**

- `SITEID` would be the study site identifier, carried through
  unchanged from the matching input field; it is present on every
  record.

**Note:** the 20-character limit applies to the joined identifier
on every record. A site identifier long enough to push the joined
value past that limit is rejected after the dataset completes, so
no artifact is accepted.

**Standard:** SDTM | **Domain:** DM

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
    str_concat:
      sources:
        - source: DM_RAW.STUDYID
        - literal: '-'
        - source: DM_RAW.SITEID
        - literal: '-'
        - source: DM_RAW.SUBJID
  verifications:
    - max_length:
        max: 30
```

Do not lower the bound below the values the study produces merely to make
this input pass. A subject identifier that no longer fits its stated width is
a defect wherever it is carried.
