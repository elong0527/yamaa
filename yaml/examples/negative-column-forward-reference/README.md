# ADaM ADLB: reject a doubled value built from a later column

This example doubles a collected laboratory result into one record per
measurement:

- `AVAL` is the collected result in standard units.
- `AVALDOUBLED` is twice the collected result.

The doubled value is listed before the collected result it doubles, but
values complete in listed order, so the doubled value has nothing to read
when its turn comes. The run must fail before any data is read and no
artifact is produced.

## How to fix

List `AVAL` before `AVALDOUBLED`, so every value reads entries the column
phase has already completed.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-column-forward-reference.html)
