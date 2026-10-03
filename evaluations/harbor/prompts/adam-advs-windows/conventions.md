Following CDISC ADaM standards, use the provided SDTM VS dataset to
create an ADVS dataset with one record per measurement, plus one
expected record for each planned analysis visit with no measurement.
Derive PARAMCD from VSTESTCD, ADT from VSDTC, ADY from VSDY, and AVAL
from VSSTRESN; then window on ADY as specified below.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
AVISIT, AVISITN, ANL01FL

AVISIT is SCREENING, BASELINE, WEEK 2, WEEK 4, or POST-TREATMENT, or has no
value. AVISITN is -1, 0, 2, 4, or 99 in that order. ANL01FL is Y or has no
value.

An expected record carries the planned visit name and number, the
window's AVISIT and AVISITN, a continued VSSEQ, and no date, day, or
value; it never takes ANL01FL. POST-TREATMENT never gets one.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
