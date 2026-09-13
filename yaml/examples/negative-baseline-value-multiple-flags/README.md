# ADaM ADLB: reject a baseline carried from two flagged visits

This example uses laboratory records and a baseline flag with a `yamaa`
specification to derive one row per visit:

- `BAVL` is the subject's screening result, repeated on every row so later
  visits compare against the same starting point. When two visits carry the
  flag, no single starting point exists.

## How to fix

Keep exactly one baseline visit per subject. Either correct the flag on the
visit that is not the baseline:

```yaml
LBBLFL: " "
```

or, when both visits genuinely qualify, pick one by rule (for example the
earliest) and flag only that row before deriving.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-baseline-value-multiple-flags.html)
