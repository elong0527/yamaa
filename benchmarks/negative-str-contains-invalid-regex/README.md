# Reject Invalid Regex in str_contains

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-contains-invalid-regex.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose term mentions severe intensity.

**Input:** collected adverse events with one record for each event,
carrying the event term.

**Variables:**

- `AESEV` would hold `SEVERE` when the event term contains `SEVERE`
  anywhere, and `MILD` otherwise, including when the term is missing.

The pattern `SEVERE[` is not a valid portable regex: the `[` opens a
character class that never closes. The request is rejected before any
data is read, and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide what the term must contain. To flag a term that mentions severe
intensity, as the goal states, remove the bracket:

```yaml
when: "str_contains(AE.AETERM, 'SEVERE')"
```

Only if the literal text `SEVERE[` is meant, escape the bracket:

```yaml
when: "str_contains(AE.AETERM, 'SEVERE\\[')"
```
