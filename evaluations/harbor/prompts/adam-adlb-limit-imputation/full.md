Following CDISC ADaM standards, use the provided LB dataset to create an
ADLB dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LBSEQ, PARAMCD, AVALC, AVAL, AVALU, DTYPE

AVALU is the LBSTRESU value, "copies/mL" on every record.

AVALC is the reported result text; it has no value when no result was
reported. A reported result is a nonnegative decimal number, optionally
preceded by "<", ">", "<=", ">=", or "=". AVAL is the numeric value of
the reported result:
- for a result starting with "<" but not "<=", AVAL is its number
  minus 1, and for a result starting with ">" but not ">=", AVAL is
  its number plus 1;
- for a result starting with "<=" or ">=", AVAL is its number itself;
- for an ordinary number or a result starting with "=", AVAL is its
  number itself.
AVAL has no value when no result was reported, and a reported zero
keeps the value 0. AVAL is not rounded.

DTYPE is "IMPUTATION" for a result given as an inequality, starting with
"<" or ">", and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
