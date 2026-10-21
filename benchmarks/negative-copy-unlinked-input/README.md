# Reject subject-level variables read from a file without its producer

**Goal:** give every adverse event record its subject's age (`AGE`) and sex
(`SEX`) from the subject-level analysis dataset (ADSL).

**Input:** adverse event records carrying the reported term (`AETERM`), and
the ADSL file the study's subject-level specification saves, read as a plain
file that does not name the specification that produced it.

**Variables:**

- `AGE` and `SEX` would be the subject's values in ADSL, repeated on each of
  their events.

**Note:** a variable taken from ADSL keeps the name, type and label the
specification that builds ADSL declares, so that specification must be
named. A file read on its own states none of them, so the run is refused
before any record is read.

## How to fix

Name the specification that produces the ADSL file in the input's `schema`,
so its declared variables type both the file and the copied columns:

```yaml
input:
  ADSL:
    path: adsl.csv
    schema: spec_adsl.yaml
    key: [STUDYID, USUBJID]
```

A dataset that cannot name its producer declares each variable itself, with
its own type and label, reading `ADSL.AGE` and `ADSL.SEX`.
