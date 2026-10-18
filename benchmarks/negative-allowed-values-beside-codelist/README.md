# Reject a Second List of Sex Codes

**Goal:** carry the collected sex (`SEX`) for each subject, coded with
the study's controlled terminology for sex.

**Input:** collected demographics with the study and subject
identifiers and the recorded sex code.

**Variables:**

- `SEX` would be the subject's recorded sex, one of the codes the
  terminology lists: `F`, `M` or `U`.

**Note:** the accepted codes are stated once. The terminology the
column is bound to already lists every accepted code, so a second list
of the same codes is rejected before any record is read, even when the
two lists agree.

## How to fix

Remove the `allowed_values` check. The bound `SEX` codelist lists its
items, and every non-missing value must equal one of them:

```yaml
- name: SEX
  type: str
  label: Sex
  derivation: DM_RAW.SEX
  submission:
    codelist: SEX
    origin:
      type: Collected
```

To accept fewer codes than the published list, bind a sponsor codelist
that lists only those codes instead of narrowing the binding with a
second list.
