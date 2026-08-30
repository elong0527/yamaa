# ADaM ADSL: reject duplicate subject enrichment

This example uses one demographics record and two analysis-subject records for
the same subject to attempt one output record:

- `TRT01A` is meant to carry the subject's actual treatment from the
  analysis-subject source.

The subject keys identify two possible source records and no selection rule is
declared. Choosing either treatment would be arbitrary, so the run must fail
and no artifact is accepted.

## How to fix

Reconcile the analysis-subject source so each subject key has one supported
treatment. If multiple records are legitimate, add a field that expresses the
choice, such as an effective timestamp, and select by it explicitly:

```yaml
source:
  variable: ADSL_RAW.TRT01A
  multiple_matches:
    order_by: [ADSL_RAW.EFFECTIVEDTC]
    keep: last
```

Do not use file order or treatment text as a substitute for a business rule.
