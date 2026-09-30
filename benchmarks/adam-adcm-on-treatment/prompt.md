Following CDISC ADaM standards, use the provided CM and ADSL datasets to
create an ADCM dataset with one record per medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, ASTDT, AENDT, TRTSDT, TRTEDT, ONTRTFL

ASTDT and AENDT are the medication's analysis start and end dates.
TRTSDT and TRTEDT are the subject's first and last treatment dates.
ONTRTFL is Y when the medication dates overlap the treatment period,
and has no value otherwise. A medication ending before treatment
starts, or starting after treatment ends, is left unflagged. A
medication with a missing start or end date is assumed to overlap
unless its known dates rule overlap out. A missing treatment end date
leaves the period open-ended, while a subject with no treatment start
date is never flagged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
