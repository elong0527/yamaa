# Reject Unmapped Value

**Goal:** translate the collected overall response into its
analysis category (`AVALC`).

**Input:** response records carrying the collected overall
response (`OVRLRESP`).

**Variables:**

- `AVALC` would contain the analysis category matching the
  collected overall response: `CR` means `COMPLETE RESPONSE`, `PR`
  means `PARTIAL RESPONSE`, `SD` means `STABLE DISEASE`, and `PD`
  means `PROGRESSIVE DISEASE`. Every response must be present and
  listed; the collected value `NE` is not, so the run is rejected
  with no artifact accepted.

## How to fix

If `NE` is a valid collected response, add its governed analysis meaning to
the dictionary:

```yaml
mapping:
  source: RS.OVRLRESP
  dict:
    CR: COMPLETE RESPONSE
    PR: PARTIAL RESPONSE
    SD: STABLE DISEASE
    PD: PROGRESSIVE DISEASE
    NE: NOT EVALUABLE
```

If an unknown response should instead produce a missing result, state that
missing-value answer explicitly with `unmapped: null`. Prefer completing the
dictionary when the value is valid.
