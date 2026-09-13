# Reject an overall response with no analysis meaning

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-mapping-unmapped-value.html)

**Goal:** translate the collected overall response into its
analysis category (`AVALC`).

**Input:** response records carrying the collected overall
response (`OVRLRESP`).

**Variables:**

- `AVALC` would contain the analysis category matching the
  collected overall response: `CR` means `COMPLETE RESPONSE`, `PR`
  means `PARTIAL RESPONSE`, `SD` means `STABLE DISEASE`, and `PD`
  means `PROGRESSIVE DISEASE`. The collected value `NE` matches no
  entry and no fallback is stated, so the run is rejected with no
  artifact accepted.

**Standard:** ADaM | **Domain:** ADRS

## How to fix

If `NE` is a valid collected response, add its governed analysis meaning to
the dictionary:

```yaml
mapping:
  source: RS.OVRLRESP
  dict:
    NE: NOT EVALUABLE
```

If an unknown response should instead produce a missing result, state that
missing-value answer explicitly. Prefer completing the dictionary when the
value is valid.
