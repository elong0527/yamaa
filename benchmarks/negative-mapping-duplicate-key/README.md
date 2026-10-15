# Reject Duplicate Key

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

## How to fix

Make the lookup table unique on `[LBTESTCD, SEX]` by resolving the
conflicting `ALT/F` reference limits under the study's governed
reference-range rules. A lookup cannot choose one duplicate by file order.

If both rows are valid for different conditions, such as different assay
methods, carry that condition on both the collected records and the table,
and add it to the key, pairing the table column with the collected value. A
collected field that is not an output column is named with its dataset prefix:

```yaml
intermediates:
  - id: LBREF_LOOKUP
    dataset: LBREF
    key: {LBTESTCD: PARAMCD, SEX: SEX, METHOD: LB.METHOD}
```
