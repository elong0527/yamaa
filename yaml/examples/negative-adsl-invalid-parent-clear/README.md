# ADaM ADSL: reject removal of a required variable property

This example attempts to prepare subject records while removing information
needed to interpret a subject identifier.

The resulting identifier would have no declared value kind, so the run must
fail before any source data is read.

## How to fix

Omit `type` to inherit it unchanged, or replace it with a complete valid value.
Only optional immediate fields may use `null` to clear an inherited value.
