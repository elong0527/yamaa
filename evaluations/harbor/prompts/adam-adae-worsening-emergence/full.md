Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDTM, AENDTM, TRTSDTM, AESEV, AETOXGR,
TRTEMFL

ASTDTM and AENDTM copy AESTDTC and AEENDTC, the collected onset and end
moments; each stays empty when its source is missing. TRTSDTM is the
moment of first exposure from ADSL. AETOXGR holds the collected toxicity
grade from 1 through 5, and is empty when the event was never graded.

Within each STUDYID, USUBJID and AEDECOD, order episodes by ASTDTM
(missing last), then numeric AESEQ. Consecutive episodes belong to the
same continuous chain only when the preceding AENDTM equals the next
ASTDTM exactly. A gap, overlap or missing boundary breaks the chain.
An unchanged episode can link an earlier event to later worsening.

TRTEMFL holds Y when the event started at or after first exposure, or
when it started before and a later episode in its continuous chain
starts at or after exposure with a higher severity (MILD < MODERATE <
SEVERE) or toxicity grade than this event. A missing severity or grade
on either side shows no worsening for that dimension. TRTEMFL has no
value for an earlier event without such worsening, and when the onset
or first exposure is missing. Untreated subjects' events are never
flagged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
