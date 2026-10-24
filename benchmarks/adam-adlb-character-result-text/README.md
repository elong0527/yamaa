# Analysis value as fixed-decimal text

**Goal:** write each analysis value (`AVAL`) as display text, adding
`AVALC` and `AVALCS`.

**Input:** laboratory (LB) records carrying one numeric analysis value per
subject and test.

**Variables:**

- `AVALC`: the analysis value rounded to two decimals and written with
  exactly two, trailing zeros kept, right-aligned in eight characters with
  leading spaces; a value too wide for eight characters is written whole,
  and a value that was not collected is written NOT DONE.
- `AVALCS`: the same value rounded to one decimal and written with exactly
  one, without leading spaces; blank when the value was not collected.

**Note:** the rounding is the one the file-level display precision uses: a
half goes away from zero, and so does a value stored a hair below a half,
so 2.675 shows as 2.68. A value that rounds to zero shows no minus sign.
Unlike a number reported to fixed places as the file is written, this text
is part of the dataset, for display only: nothing downstream reads it back
as a number, and the analysis value keeps every digit it was collected with.
