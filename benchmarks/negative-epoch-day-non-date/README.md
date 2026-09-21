# Epoch Day Non-Date

[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** fix the failure behavior of `to_epoch_day` when the source
is not a date.

**Input:** exposure records with a text start date (`EXSTDT` as str,
not date).

**Expected:** validation fails with `incompatible_input_type`
because `to_epoch_day` requires a `date` input (REQ-0606). A text
value must first be converted via `to_date`.
