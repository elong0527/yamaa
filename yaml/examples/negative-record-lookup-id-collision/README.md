# ADaM ADSL: reject a first treatment named after its own source

This example uses collected demographics with exposure records to attempt one
record per subject:

- `TRT01A` is the treatment the subject received first.

The chosen record is given the name the exposure records already have, so
`EX.EXTRT` no longer says whether it means the treatment of one chosen record
or of all of them. The two readings differ for every subject with more than one
record, so the run must fail and no artifact is accepted.

## How to fix

Give the record lookup an identifier that is distinct from every dataset and
from the output domain, then read through that identifier:

```yaml
record_lookups:
  - id: FIRSTEX
    dataset: EX
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first

# ...
derivation:
  source: FIRSTEX.EXTRT
```
