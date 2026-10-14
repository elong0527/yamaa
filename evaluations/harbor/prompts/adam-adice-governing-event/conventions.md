Following CDISC ADaM standards, use the provided ADSL and ICE_RAW datasets
to create an ADICE dataset with one record per intercurrent event per
subject, summarizing the governing event in the completed event history.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADECOD1, ASTDT, ICEPRIO, ICERANK, GOVFL, SUPERFL,
GOVTYPE, GOVDT, GOVSEQ

ICEPRIO is 1 for ICE04 and ICE05, 2 for ICE01, 3 for ICE02 and 4 for ICE03.
ICERANK numbers each subject's events from 1 by increasing ICEPRIO, then
earliest ASTDT, then lowest ASEQ. GOVFL and SUPERFL are "Y" or have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adice.csv.
