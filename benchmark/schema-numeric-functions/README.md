# Numeric Functions

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-numeric-functions.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** apply `CEIL`, `FLOOR`, `TRUNC`, `EXP`, `MOD`,
`GREATEST`, `LEAST`, and `COALESCE` to carried values.

**Input:** subject-level records carrying `AVAL` as a decimal
number and `BVAL` and `CVAL` as whole numbers.

**Variables:**

- `AVAL` holds the decimal number read from the source; blank
  when the source carried nothing.
- `BVAL` holds the whole number read from the source; blank when
  the source carried nothing.
- `CVAL` holds the second whole number read from the source;
  blank when the source carried nothing.
- `CEILVAL` holds the smallest whole number at or above `AVAL`;
  blank when `AVAL` is blank.
- `FLOORVAL` holds the largest whole number at or below `AVAL`;
  blank when `AVAL` is blank.
- `TRUNCVAL` holds `AVAL` with its fraction cut away toward
  zero; blank when `AVAL` is blank.
- `EXPVAL` holds the constant e raised to `AVAL`; blank when
  `AVAL` is blank.
- `MODVAL` holds the remainder of `BVAL` divided by three, with
  the sign of `BVAL`; blank when `BVAL` is blank.
- `GREATESTVAL` holds the largest among `BVAL`, `CVAL`, and
  zero, skipping blanks; blank only when all three are blank,
  which the zero here prevents.
- `LEASTVAL` holds the smallest among `BVAL`, `CVAL`, and zero,
  skipping blanks; blank only when all three are blank, which
  the zero here prevents.
- `COALESCEVAL` holds `CVAL` when present and otherwise `BVAL`;
  blank when both are blank.

**Note:** a blank input gives a blank result, except that the
largest-or-smallest choice skips blanks and the first-present
choice moves to the next value.

**Standard:** ADaM | **Domain:** ADSL
