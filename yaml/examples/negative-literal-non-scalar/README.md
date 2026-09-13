# Reject a fixed value written as a structure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-literal-non-scalar.html)

**Goal:** carry age into `AGE` and group subjects into `AGEGRP`,
with the older group holding `elderly` and the younger group
holding `younger`.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGEGRP` would contain `elderly` for subjects aged 65 or older
  and `younger` for younger subjects.

The older group's fixed value is written as a structure instead of
a single value, and no reader may guess which part names the
group, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide the exact text the group carries, then write that single value. When
the older group reads as one word, state the word:

```yaml
- when: "AGE >= 65"
  then: {literal: elderly}
```

When the value genuinely has parts, each part needs its own result column
rather than sharing one.
