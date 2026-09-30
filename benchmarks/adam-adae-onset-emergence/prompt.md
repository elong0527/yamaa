Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TRTEMFL, AOCCFL

ASTDTM is the collected onset moment and TRTSDTM is the moment of first
exposure from ADSL. TRTEMFL is Y when the event started at or after
first exposure. It has no value for an earlier event, and when either
the onset or the first exposure is missing. Emergence is decided at
the moment, not the day: events starting earlier and later on the day
of first exposure fall on opposite sides.

AOCCFL is Y on the subject's earliest treatment-emergent event, ordered
by onset moment with the lower AESEQ settling ties at the same second.
All other rows have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
