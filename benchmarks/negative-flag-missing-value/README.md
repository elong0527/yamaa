# Reject Flag Without Missing Value

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y` and
every younger subject with `N`.

**Input:** collected demographics carrying numeric age (`AGE`), including
one subject with no recorded age.

**Variables:**

- `AGE` is the collected age, missing where it was not collected.
- `ELDFL` would be `Y` for subjects aged 65 or older and `N` for younger
  subjects.

**Note:** the flag names `N` for a false condition but not what a subject
with no recorded age receives, so the run is rejected before any data is
read and no artifact is accepted.

## How to fix

State what an unknown age returns. To give `N` to every subject not known
to be 65 or older:

```yaml
flag:
  condition: AGE >= 65
  false_value: N
  missing: N
```

Write `missing: null` instead to leave the flag missing when the age is
unknown.
