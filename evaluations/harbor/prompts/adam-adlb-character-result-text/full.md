Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, AVALC, AVALCS

AVAL keeps every digit it was collected with and is not rounded. AVALC and
AVALCS are display text of AVAL, held in the dataset:

- AVALC is AVAL rounded to two decimals and written with exactly two,
  trailing zeros kept, right-aligned in eight characters with leading
  spaces; a minus sign counts as one of the eight. A value too wide for
  eight characters is written whole, with both decimals and no leading
  spaces, never shortened. When AVAL has no value (the value was not
  collected), AVALC is "NOT DONE".
- AVALCS is AVAL rounded to one decimal and written with exactly one,
  without leading spaces. AVALCS has no value when AVAL has no value.
- Both are plain positional numbers: no digit grouping, no exponent, and
  one 0 before the point for a value below one in magnitude.

A half rounds away from zero, and so does a value stored a hair below a
half (within 2^-26 of one unit in the last kept place), so 2.675 shows as
2.68. A value that rounds to zero shows no minus sign.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
