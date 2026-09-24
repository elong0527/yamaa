# Reject Duplicate Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-mapping-duplicate-key.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry sex and the numeric result into the output and
choose the upper limit of normal (`ANRHI`) from a reference table
by test and sex.

**Input:** collected laboratory records carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal for that test and
  sex, taken from the reference table row matching the collected
  test code and sex.

**Note:** a result whose test and sex match more than one line of
the reference table has no single line to take its limit from.
Taking either line, or the first the file happens to list, would
make the result depend on file order rather than on the study's
reference ranges, so the run is rejected with no artifact accepted,
even when the lines agree.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Make the lookup table unique on `[LBTESTCD, SEX]` by resolving the
conflicting `ALT/F` reference limits under the study's governed
reference-range rules. A lookup cannot choose one duplicate by file order.

If both rows are valid for different conditions, such as different assay
methods, carry that condition on both the collected records and the table,
and add it to both lists in the same position. A collected field that is not
an output column is named with its dataset prefix:

```yaml
lookup:
  key_base: [PARAMCD, SEX, LB.METHOD]
  dataset: LBREF
  key: [LBTESTCD, SEX, METHOD]
  value: ANRHI
```
