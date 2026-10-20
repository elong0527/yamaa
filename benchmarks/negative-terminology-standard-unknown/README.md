# Reject a Terminology Standard Define-XML Does Not Name

**Goal:** carry the collected sex (`SEX`) for each subject, coded with
the study's controlled terminology for sex.

**Input:** collected demographics with the study and subject
identifiers and the recorded sex code.

**Variables:**

- `SEX` would be the subject's recorded sex, one of the codes the
  terminology lists: `F`, `M` or `U`.

**Note:** the sex terminology names the published standard it is drawn
from. That name, `CDISC CT`, is not one of the standard names Define-XML
publishes, so the study's terminology is rejected as soon as it is read,
before any record is read and before any submission document is
generated.

## How to fix

Name the published terminology as Define-XML spells it: CDISC controlled
terminology is `CDISC/NCI`.

```yaml
codelists:
  - standard:
      name: CDISC/NCI
      publishing_set: SDTM
      version: '2023-12-15'
```

A list the sponsor defines, rather than draws from published terminology,
belongs in a codelist source that names no `standard`.
