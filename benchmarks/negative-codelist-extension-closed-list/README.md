# Reject a Sponsor Addition to a Closed Published Codelist

**Goal:** build the demographics domain with each subject's death flag
(`DTHFL`), restricted to the study's no-yes codelist.

**Input:** collected demographics, one record per subject, and the study's
codelists in the project file: the published no-yes response list.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `DTHFL` would be `Y` for a subject who died; missing otherwise.

**Note:** the sponsor added an unknown answer, `U`, to the published no-yes
list. That list admits no additions: its published values are the whole
list, so the addition contradicts the list it claims to extend, and the run
is rejected before any data is read; no artifact is accepted.

## How to fix

Keep the published list as published. When the study needs an unknown
answer, declare a sponsor list of its own, without a published standard,
and bind the column to it:

```yaml
codelists:
  - codelists:
      - id: NYU
        name: No Yes Unknown Response
        items:
          - value: N
          - value: Y
          - value: U
```
