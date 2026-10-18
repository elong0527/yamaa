Following CDISC ADaM standards, use the provided ADSL, DS, and RS datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "PFS" and PARAM is "Progression-Free Survival" for every record.
STARTDT is the subject's randomization date, RANDDT in ADSL. The progression
date is the earliest date of an overall response assessment (RSTESTCD
"OVRLRESP") with a result of "PD" (RSSTRESC) that is flagged adequate (ADEQFL
"Y") and has a date; when assessments share that date, the one with the
lowest sequence number is the progression record. The death date is the
earliest date of a disposition record with an outcome of "DEATH" (DSDECOD),
the record with the lowest sequence number supplying it when records share
that date. Only assessments flagged adequate can supply a progression event
or a censoring date, and an assessment without a date can supply neither, so
an inadequate progression assessment leaves the subject censored at the last
adequate dated assessment. When the subject started new anti-cancer therapy
(NTXSTDT in ADSL), only dates on or before the therapy start date are
usable: a progression or death dated after the therapy start date is not an
event, and censoring uses only adequate assessments dated on or before the
therapy start date. ADT is the earlier of the progression and death dates
when either is present, and CNSR is 0 then; a death after the last adequate
assessment is still an event. When neither date is present, CNSR is 1 and
ADT is the date of the subject's last adequate dated overall response
assessment. A subject with no usable adequate assessment at all is censored
at the randomization date: ADT is STARTDT, AVAL is 1, and SRCDOM, SRCVAR,
and SRCSEQ have no value. AVAL is the number of days from STARTDT through
ADT, counting the randomization day as day one. EVNTDESC is "DISEASE
PROGRESSION" for a progression event, "DEATH" for a death event, and
"CENSORED" when both dates are absent; a progression and a death on the same
day count as progression. SRCDOM, SRCVAR, and SRCSEQ trace ADT to its
source: "DS", "DSDTC", and the disposition sequence number for a death
event, or "RS", "RSDTC", and the response sequence number for a progression
event or a censored assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
