Following CDISC ADaM standards, use the provided SDTM VS dataset to
create an ADVS dataset with one record per measurement, plus one
expected record for each planned analysis visit with no measurement.
Derive PARAMCD from VSTESTCD, ADT from VSDTC, ADY from VSDY, and AVAL
from VSSTRESN; then window on ADY as specified below.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, VSSEQ, VISIT, VISITNUM, ADT, ADY, AVAL,
AVISIT, AVISITN, ANL01FL

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
