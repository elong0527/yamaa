Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

Each LB record is a collected record in the output. On a collected record,
PARAMCD is the test code, PARAM is the test name, AVAL is the collected
result, and DTYPE has no value. On an added absolute record, AVAL is the WBC
count times the matching differential fraction for the same subject and
visit, and DTYPE is "CALCULATION". LYMPH is the added absolute parameter for
the LYMLE fraction, with PARAM "Lymphocytes Abs (10^9/L)". NEUT is the added
absolute parameter for the NEUTLE fraction, with PARAM "Neutrophils Abs
(10^9/L)". MONO is the added absolute parameter for the MONOLE fraction,
with PARAM "Monocytes Abs (10^9/L)". EOS is the added absolute parameter
for the EOSLE fraction, with PARAM "Eosinophils Abs (10^9/L)". BASO is the
added absolute parameter for the BASOLE fraction, with PARAM "Basophils Abs
(10^9/L)". An absolute record is added for a subject, visit, and
differential only when both a WBC count and that differential's fraction
are present for the subject and visit and no record of that absolute
parameter is already there. A differential with a fraction but no WBC count
gains nothing, and an existing absolute record blocks only its own
differential.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
