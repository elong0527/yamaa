Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

The differential fraction codes are "LYMLE", "NEUTLE", "MONOLE",
"EOSLE", and "BASOLE"; the absolute parameter codes and names are
"LYMPH" / "Lymphocytes Abs (10^9/L)", "NEUT" / "Neutrophils Abs
(10^9/L)", "MONO" / "Monocytes Abs (10^9/L)", "EOS" / "Eosinophils
Abs (10^9/L)", and "BASO" / "Basophils Abs (10^9/L)". DTYPE is
CALCULATION or empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
