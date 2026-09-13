# Reject a sex code checked against an unreadable text rule

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-matches-unreadable-pattern.html)

**Goal:** record `SEX` for each subject, accepting only `M`, `F`,
and `U`.

**Input:** collected demographics carrying subject sex (`SEX`).

**Variables:**

- `SEX` would be the subject's sex, copied from the collected
  records, holding `M`, `F`, or `U`.

The text rule the check is written in cannot be read, so the run
is rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which codes the study collects, then write the check in the one
notation every reader uses. A named group is spelled `(?<name>...)` rather
than `(?P<name>...)`, and a check that must describe the whole value anchors
itself:

```yaml
- matches:
    pattern: '^[MFU]$'
```

When the permitted set is small and fixed, list it instead. The list needs no
text rule at all and cannot drift from the codes it names:

```yaml
- allowed_values:
    values: [M, F, U]
```
