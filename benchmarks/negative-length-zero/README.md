# Reject a Site Identifier With a Length of Zero

**Goal:** build the demographics domain with each subject's study site
(`SITEID`), and the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SITEID` would be the identifier of the subject's study site.

**Note:** the site identifier declares a length of 0, a placeholder that was
never filled in. A length is the longest value the variable holds, so it is
a positive number; no site identifier fits in zero characters, and the run
is rejected before any data is read and no artifact is accepted.

## How to fix

State the longest site identifier the study assigns:

```yaml
submission:
  core: Req
  role: Record Qualifier
  length: 3
  origins:
    - type: Assigned
      source: Sponsor
```
