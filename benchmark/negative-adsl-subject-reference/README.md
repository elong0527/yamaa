# Reject a malformed subject reference

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adsl-subject-reference.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** build a subject display reference (`SUBJREF`) from the
collected site and subject identifiers.

**Input:** demographics (DM) records carrying the collected site
identifier (`SITEID`) and the subject identifier for the study
(`SUBJID`).

**Variables:**

- `SUBJREF` would contain the site and subject identifiers joined
  by a colon (for example `101:0007`), but no row is produced.

The braced text holds several inputs and punctuation instead of
one input name. Reading that text as instructions would make its
meaning ambiguous, so the run is rejected before any data is read,
and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Put punctuation outside the placeholders and give each placeholder
exactly one variable name:

```yaml
derivation:
  str_template: "{SITEID}:{SUBJID}"
```

This produces values such as `101:0007` without evaluating code
embedded in the template.
