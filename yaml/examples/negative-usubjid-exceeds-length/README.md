# SDTM DM: reject a subject identifier longer than the study permits

This example uses collected demographics to attempt one record per subject:

- `USUBJID` is the unique subject identifier, built from the study, the site,
  and the subject number.

The study fixes how long a subject identifier may be, because every dataset
that refers to a subject repeats the same value and a submission states its
width once. One site's identifier is long enough that the value built for its
subjects exceeds that width. Cutting the value to fit would break the link to
every other dataset carrying it, and keeping the longer value would contradict
the width already stated, so the run must fail and no artifact is accepted.

## How to fix

Decide which is authoritative: the width the study declares, or the site
identifier the data carries. If the site identifier can be shortened at
source, correct it there, so that every dataset referring to the subject keeps
one value:

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

Do not lower the bound below the values the study produces merely to make this
input pass. A subject identifier that no longer fits its stated width is a
defect wherever it is carried.
