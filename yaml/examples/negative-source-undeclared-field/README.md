# ADaM ADSL: reject a copy from an undeclared field

This example uses collected demographics to record one row per subject:

- `AGE` is the age at enrollment, copied from the governed source.

The copy names a field the source does not declare. No implementation
may guess which collected field was meant, so the specification is
rejected before any data is read and no artifact is accepted.

## How to fix

Decide which collected field holds the age, then name it exactly. When
the source carries the age under its own name, copy that field:

```yaml
- name: AGE
  type: int
  derivation:
    source: DM.AGE
```

When the source truly lacks the field, add it to the governed source
first rather than pointing at a name nothing declares.
