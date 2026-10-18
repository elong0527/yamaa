Following CDISC ADaM standards, use the provided ADSL and OE datasets to create
an ADOE dataset with one record per collected ophthalmic examination
measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE

AFEYE is "Both Eyes", "Study Eye", "Fellow Eye", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adoe.csv.
