# Reject Malformed Reference

**Goal:** build a subject display reference (`SUBJREF`) from the
collected site and subject identifiers.

**Input:** demographics (DM) records carrying the collected site
identifier (`SITEID`) and the subject identifier for the study
(`SUBJID`).

**Variables:**

- `SUBJREF` would contain the site and subject identifiers joined
  by a colon (for example `UCSD:0001`).

The braced text holds several inputs and punctuation instead of
one input name. Reading that text as instructions would make its
meaning ambiguous, so the run is rejected before any data is read,
and no artifact is accepted.

## How to fix

Put punctuation outside the placeholders and give each placeholder
exactly one variable name:

```yaml
derivation:
  str_template: "{SITEID}:{SUBJID}"
```

This produces values such as `UCSD:0001` without evaluating code
embedded in the template.
