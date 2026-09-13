# ADaM ADSL: reject a fixed value written as a structure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-literal-non-scalar.html)

This example uses collected demographics to record one row per subject:

- `AGE` is the age at enrollment.
- `AGEGRP` groups subjects into an older and a younger group.

The older group's fixed value arrives as a structure instead of a
single scalar. No implementation may guess which part of the structure
the group name is, so the specification is rejected before any data is
read and no artifact is accepted.

## How to fix

Decide the exact text the group carries, then write that scalar. When
the older group reads as one word, state the word:

```yaml
- when: "AGE >= 65"
  then: {literal: elderly}
```

When the value genuinely has parts, each part needs its own result
column rather than sharing one.
