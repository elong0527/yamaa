# ADaM ADSL: reject a site-scoped subject identity

This example uses collected demographics to attempt one record per subject:

- `INVID` is the investigator responsible for the subject's site.

Each record is meant to be identified by its study, subject, and site, but the
site is used only while deriving and is not part of the result. An identity
that depends on a value the result does not carry cannot be checked by a reader
of the result, so the run must fail and no artifact is accepted.

## How to fix

Every key column must be present in the output. If `SITEID` is part of the
record identity, expose it by removing `output: false`:

```yaml
- name: SITEID
  type: str
  derivation:
    source: DM.SITEID
```

If study and subject already form the intended unique identity, remove
`SITEID` from `keys` instead. Choose the option that matches the output's
actual grain.
