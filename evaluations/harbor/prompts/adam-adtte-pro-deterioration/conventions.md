Following CDISC ADaM standards, use the provided ADSL, QS, and DS datasets
to create an ADTTE dataset for time to PRO deterioration, one record per
subject.

The ADTTE dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ.

Use PARAMCD "TTDGHS" and PARAM "Time to Deterioration in Global Health
Status". A deterioration is dated after the earliest assessment and scores
at least 10 points below it. The event is the first deterioration, or death
without a prior one; neither counts when it falls after the first censoring
reason. Progression, discontinuation, and withdrawal are censoring reasons,
never events. AVAL counts whole calendar months from STARTDT to ADT; CNSR
is 0 for an event and 1 otherwise. EVNTDESC is PRO DETERIORATION, DEATH, or
CENSORED; CNSDTDSC holds the censoring reason (STUDY COMPLETION when no
reason is recorded). Trace ADT as QS/ADT/sequence, ADSL/DTHDT, or ADSL/RANDDT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
