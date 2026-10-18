Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

DTYPE is "CALCULATION" or has no value.

LYMPH is the added absolute parameter for the LYMLE fraction, with PARAM
"Lymphocytes Abs (10^9/L)". NEUT is the added absolute parameter for the
NEUTLE fraction, with PARAM "Neutrophils Abs (10^9/L)". MONO is the added
absolute parameter for the MONOLE fraction, with PARAM "Monocytes Abs
(10^9/L)". EOS is the added absolute parameter for the EOSLE fraction,
with PARAM "Eosinophils Abs (10^9/L)". BASO is the added absolute
parameter for the BASOLE fraction, with PARAM "Basophils Abs (10^9/L)".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
