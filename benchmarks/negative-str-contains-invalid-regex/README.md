# Reject Invalid Regex in str_contains

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-contains-invalid-regex.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose term mentions severe intensity.

**Input:** collected adverse events with one record for each event,
carrying the event term.

**Variables:**

- `AESEV` would hold `SEVERE` when the event term contains the word
  `SEVERE` and `MILD` otherwise; but no row is produced because the
  pattern `SEVERE[` is not a valid portable regex (the `[` opens a
  character class that never closes), so the request is rejected
  before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Close the character class or drop the bracket. To match the literal
text `SEVERE[`, escape the bracket:

```yaml
when: "str_contains(AE.AETERM, 'SEVERE\\[')"
```

To match just the word `SEVERE`, remove the bracket:

```yaml
when: "str_contains(AE.AETERM, 'SEVERE')"
```
