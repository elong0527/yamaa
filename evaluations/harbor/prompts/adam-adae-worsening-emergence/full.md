Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDTM, TRTSDTM, AESEV, AETOXGR,
TRTEMFL

ASTDTM is the onset moment and TRTSDTM is the moment of first exposure
from ADSL. AETOXGR holds the collected toxicity grade from 1 through
5, and is empty when the event was never graded. TRTEMFL holds Y when
the event started at or after first exposure, or when it started
before but the same term reaches a higher severity or a higher
toxicity grade than this event after exposure; a missing severity or
grade, on this event or on the later ones, shows no worsening. TRTEMFL
has no value for an earlier event without later worsening, and when
the onset or the first exposure is missing. Worsening is judged within
the subject and dictionary term, and an untreated subject's events are
never flagged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
