Following CDISC ADaM standards, use the provided ADSL, DS, and QS datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "TTDGHS" and PARAM is "Time to Deterioration in Global Health
Status". STARTDT is the randomization date. A deterioration is an
assessment dated after the subject's earliest assessment whose score has
fallen at least 10 points below that earliest assessment's score. The
deterioration date is the date of the subject's earliest deterioration.
The first censoring reason is the subject's earliest disposition record
naming DISEASE PROGRESSION, STUDY DISCONTINUATION, or WITHDRAWAL OF
CONSENT, and progression, discontinuation, and withdrawal are censoring
reasons, never events. ADT is the deterioration date when a deterioration
exists and is not dated after the first censoring reason. A deterioration
and a death on the same date count as deterioration. Otherwise ADT is the
death date when the subject died and the death is not dated after the
first censoring reason. With neither, ADT is the last assessment dated
on or before the first censoring reason, or the randomization date when
no assessment qualifies. AVAL is the number of whole calendar months from
STARTDT to ADT. CNSR is 0 when the subject has a deterioration or death
event and 1 otherwise. EVNTDESC is "PRO DETERIORATION" for a
deterioration event, "DEATH" for a death event, and "CENSORED" otherwise.
CNSDTDSC has no value for an event, holds the censoring reason when one
is available, and is "STUDY COMPLETION" otherwise. SRCDOM is "QS" for a
deterioration or a censored record with a supporting assessment, and
"ADSL" for a death or a record censored at randomization. SRCVAR is
"ADT" for a deterioration or a censored record with a supporting
assessment, "DTHDT" for a death, and "RANDDT" for a record censored at
randomization. SRCSEQ is the QSSEQ of the deterioration or censoring
assessment, and has no value when the analysis date comes from a
subject-level date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
