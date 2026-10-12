Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TEENDDTM, TRTEMFL,
AOCCFL

ASTDTM is the collected onset moment, TRTSDTM is the moment of first
exposure from ADSL, and TEENDDTM is the end of the treatment-emergent
window (30 days after last exposure) from ADSL.

TRTEMFL is Y when the event started inside the window from first
exposure through the window end, inclusive. It has no value for an
event that started before first exposure or after the window end.
A missing onset cannot be ruled pre-treatment, so a treated subject's
event with a missing onset is flagged Y; a subject with no first
exposure at all is never treatment-emergent. Emergence is decided at
the moment, not the day: events starting earlier and later on the day
of first exposure fall on opposite sides.

AOCCFL is Y on the subject's earliest treatment-emergent event with a
known onset, ordered by onset moment with the lower AESEQ settling ties
at the same second. Events with missing onset are excluded from this
selection. All other rows have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
