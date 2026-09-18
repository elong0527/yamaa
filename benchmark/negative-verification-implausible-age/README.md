# Reject an implausible age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-verification-implausible-age.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** record `AGE` for every subject from the collected
demographics, accepting only ages from 18 to 100.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGE` would be the subject's age, taken from the collected age.

A collected age of `214` falls outside the accepted range of 18 to
100, so it is rejected after the dataset completes and no artifact
is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Query and correct the source age for `P7-732` if `214` is a data-entry
error, then rerun the unchanged range check. If the protocol genuinely
permits the confirmed value, revise the check boundary to the protocol's
documented limit. Do not remove or widen the check merely to make an
unconfirmed value pass.
