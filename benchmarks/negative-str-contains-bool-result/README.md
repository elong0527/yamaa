# Reject Boolean str_contains Result in a Text Column

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-contains-bool-result.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose term mentions dermatitis or
erythema, writing the structured `str_contains` result straight into
`SKIN_FLAG`.

**Input:** collected adverse events with one record for each event,
carrying the event term.

**Variables:**

- `SKIN_FLAG` would hold whether the event term contains `DERMATITIS`
  or `ERYTHEMA`, and `UNKNOWN` when the term is missing.

The structured form returns true or false, and no column type accepts
a boolean: the first record's match yields true, which has no text
form, so the run fails and no artifact is accepted. The `missing`
value is accepted; it fires whenever a missing-term row is evaluated
before any matching row, but this run never gets that far, because
the first record's match already stops it.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Map the boolean to flag values with the predicate-text form inside a
`case`, which is the spelling that stores text:

```yaml
- name: SKIN_FLAG
  type: str
  derivation:
    case:
      - when: "AE.AEDECOD IS NULL"
        then: {literal: UNKNOWN}
      - when: "str_contains(AE.AEDECOD, 'DERMATITIS|ERYTHEMA')"
        then: {literal: Y}
      - otherwise: {literal: N}
```
