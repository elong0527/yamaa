# ADaM ADSL: reject duplicate subject enrichment

This example uses one demographics record and two analysis-subject records for
the same subject to attempt one output record:

- `TRT01A` is meant to carry the subject's actual treatment from the
  analysis-subject source.

The subject keys identify two possible source records and no selection rule is
declared. Choosing either treatment would be arbitrary, so the run must fail
and no artifact is accepted.
