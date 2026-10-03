Following CDISC ADaM standards, use the provided ADLB_RAW dataset to
create an ADLB dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE

Keep every collected record unchanged, with DTYPE empty.

Add one absolute differential record per subject and visit for each
differential fraction present: "LYMLE" becomes "LYMPH" ("Lymphocytes
Abs (10^9/L)"), "NEUTLE" becomes "NEUT" ("Neutrophils Abs (10^9/L)"),
"MONOLE" becomes "MONO" ("Monocytes Abs (10^9/L)"), "EOSLE" becomes
"EOS" ("Eosinophils Abs (10^9/L)"), and "BASOLE" becomes "BASO"
("Basophils Abs (10^9/L)"). An absolute record is added only when
the visit also has a white blood cell count (WBC) and no record of
the new absolute parameter yet. Its value is the WBC count times
the fraction for the same subject and visit, and DTYPE is
CALCULATION.

A differential with a fraction but no WBC count gains nothing, and
a visit that already holds the absolute record gains none for that
differential.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
