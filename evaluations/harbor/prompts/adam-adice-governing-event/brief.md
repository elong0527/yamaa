Following CDISC ADaM standards, use the provided ADSL and ICE_RAW datasets
to create an ADICE dataset with one record per intercurrent event per
subject, summarizing the governing event in the completed event history.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT, ICEPRIO, ICERANK, GOVFL, SUPERFL,
GOVTYPE, GOVDT, GOVSEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adice.csv.
