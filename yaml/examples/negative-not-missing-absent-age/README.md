# ADaM ADSL: reject a missing age

This example uses collected demographics to record one row per subject:

- `AGE` is the age at enrollment, required for every subject.

One subject arrives with no age recorded. The required-value rule
finds the gap, so the run is rejected after the dataset completes and
no artifact is accepted.

## How to fix

Decide whether the study can proceed without the value, then either
correct the data or loosen the rule. When the age exists on the case
report form, correct it at the governed source and rerun. When absence
is genuinely possible, drop the rule and let the column stay missing:

```yaml
- name: AGE
  type: int
  derivation:
    source: DM.AGE
```

Do not fill the gap with a placeholder age, which reports a value
nobody recorded.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-not-missing-absent-age.html)
