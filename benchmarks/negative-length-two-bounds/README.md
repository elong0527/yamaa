# Reject a Subject Identifier With Two Different Length Limits

**Goal:** build the demographics domain with the unique subject identifier
(`USUBJID`), and the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `USUBJID` would be the collected unique subject identifier.

**Note:** the subject identifier is limited to 20 characters by one
declaration and to 12 by another. The submitted length and the limit the
values are checked against must be the same number, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Decide the longest identifier the study assigns, and state it once. The
length checks the values as a limit would, so the limit can go:

```yaml
- name: USUBJID
  type: str
  label: Unique Subject Identifier
  derivation: RAW.USUBJID
  submission:
    core: Req
    role: Identifier
    length: 12
    origins:
      - type: Assigned
        source: Sponsor
```
