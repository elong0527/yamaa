Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, AVALC, AVALCS

AVALC is AVAL as display text with exactly two digits after the decimal
point, trailing zeros kept, right-aligned in eight characters with leading
spaces, or "NOT DONE"; a text wider than eight characters is written in full.
AVALCS is AVAL with exactly one digit after the decimal point, without
leading spaces. Both round a value that is exactly halfway away from zero,
as its decimal digits show it, even where binary storage holds it a hair
below the half. A value that rounds to zero is written without a minus sign.
AVAL itself is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
