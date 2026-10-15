# Reject Implausible Age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-implausible-age.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's age in `AGE` from the collected
demographics, accepting only ages from 18 to 100.

**Input:** collected demographics carrying the subject's age in
`AGE`, identified by study and subject.

**Variables:**

- `AGE` would contain the subject's age, taken from the collected
  age; missing when no age was collected.

**Note:** an age below 18 or above 100 is rejected once the dataset
is complete, so no artifact is accepted; a missing age is not
rejected.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Query and correct the source age for `YAMAA-01-102` if `214` is a
data-entry error, then rerun the unchanged range check. If the
protocol genuinely permits the confirmed value, revise the check
boundary to the protocol's documented limit. Do not remove or widen
the check merely to make an unconfirmed value pass.
