# ADaM ADSL: reject an inherited artifact layout

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-inherited-output.html)

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
