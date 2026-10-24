Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, AVALC, AVALCS

AVALC is AVAL as display text: rounded to two decimal places and written
with exactly two digits after the decimal point, trailing zeros kept,
right-aligned in eight characters with leading spaces. A value whose text is
wider than eight characters is written in full, never shortened. AVALC is
"NOT DONE" when AVAL has no value. AVALCS is AVAL rounded to one decimal
place and written with exactly one digit after the decimal point, without
leading spaces; AVALCS has no value when AVAL has no value. Both round a
value that is exactly halfway away from zero, as its decimal digits show it,
even where binary storage holds it a hair below the half. A value that
rounds to zero is written without a minus sign. AVAL itself is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
