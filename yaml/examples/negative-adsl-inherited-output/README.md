# ADaM ADSL: reject an inherited artifact layout

This example attempts to prepare subject records while allowing a reusable
parent to choose which variables appear in the final dataset.

The requested dataset must own that decision explicitly, so the run must fail
before any source data is read.

## How to fix

Declare the complete `output` in the entry file. Parent files may provide a
default for reuse, but the entry must replace it explicitly:

```yaml
output:
  columns: [USUBJID]
```
