# ADaM ADSL: reject a smoking flag whose dictionary answers twice

This example uses collected smoking status to attempt one record per subject:

- `SMOKEFL` marks a subject who reported smoking.

The collected values arrive in either case, so the rule ignores case, and the
translation table then describes one collected value on two of its lines.
Keeping the line that appears first, or the one that matches exactly, would
each be a rule the specification never stated, so the run must fail and no
artifact is accepted.

## How to fix

For case-insensitive matching, retain only one dictionary entry for each folded
key. Both `Y` and `y` then resolve through `Y`:

```yaml
mapping:
  source: DM.SMOKSTAT
  case_sensitive: false
  dict:
    Y: "Y"
    N: "N"
```

Alternatively, set `case_sensitive: true` when differently cased values are
intentionally distinct and give each one an explicit meaning.
