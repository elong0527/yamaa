Following CDISC ADaM standards, use the provided OE and ADSL datasets
to create an ADOE dataset with one row per collected ophthalmic
measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE

AFEYE is Study Eye, Both Eyes, or Fellow Eye, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adoe.csv.
