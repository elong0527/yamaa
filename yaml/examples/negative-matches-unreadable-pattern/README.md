# ADaM ADSL: reject a sex code checked against an unreadable match rule

This example uses collected demographics to record one row per subject:

- `SEX` is the subject's sex, constrained to the letters `M`, `F`, and `U`.

The rule constraining `SEX` is written in a text-matching notation this
language cannot read. Two implementations would disagree about whether the
notation is even well formed, so the specification is rejected before any data
is read and no artifact is accepted.

## How to fix

Decide which codes the study collects, then write the constraint in the one
notation every implementation reads. A named group is spelled `(?<name>...)`
rather than `(?P<name>...)`, and a constraint that must describe the whole
value anchors itself:

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
