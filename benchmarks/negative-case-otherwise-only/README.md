# Reject Flag With Only a Fallback Value

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y` and
every other subject with `N`.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `AGE` is the collected age, missing where it was not collected.
- `ELDFL` would be `Y` for subjects aged 65 or older and `N` for everyone
  else, including a subject with no recorded age. Only the value for
  everyone else is written and nothing states who receives `Y`, so the run
  is rejected before any data is read and no artifact is accepted.

## How to fix

State the condition that marks a subject as elderly, then the value for
everyone else as the last item:

```yaml
case:
  - when: "AGE >= 65"
    then: "'Y'"
  - otherwise: "'N'"
```

When every subject genuinely takes the same value, write that value alone,
`derivation: "'N'"`, without `case`.
