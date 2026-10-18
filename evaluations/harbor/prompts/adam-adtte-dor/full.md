Following CDISC ADaM standards, use the provided ADRS_RAW, ADSL, and DS
datasets to create an ADTTE dataset with one record per responder.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "DOR" and PARAM is "Duration of Response" on every record.
Only subjects whose RESPDT is not blank contribute a record. STARTDT is
the subject's response date, RESPDT in ADSL. ADT is the event date when
a counted event exists; otherwise ADT is the earlier of the last valid
tumour assessment date and the start date of the new anti-cancer
therapy. The progression date is the earliest assessment date with a
"PD" response, and an assessment with a blank date never counts as the
progression date or as the last valid assessment. The death date is the
earliest disposition date with a "DEATH" term. The event date is the
earlier of the progression date and the death date; it counts only when
it does not fall after the therapy start date, and an event on the
therapy start date itself still counts. The last valid tumour
assessment date is the latest assessment date with a response other
than "NE". When several records share the progression date, the record
with the lowest ASEQ is the progression record, and when several records
share the last valid assessment date, the record with the highest ASEQ
is the last valid assessment record. AVAL is the number of days from
STARTDT through ADT, counting both endpoints, and is at least 1. CNSR
is 0 when a counted event exists and 1 when the record is censored.
EVNTDESC is "CENSORED" when no event counts, "DISEASE PROGRESSION" when
the event date is the progression date, and "DEATH" otherwise, so a
progression and a death on the same day give "DISEASE PROGRESSION".
CNSDTDSC has no value for an event; for a censored record it is "LAST
TUMOUR ASSESSMENT" when the censoring date is the last valid tumour
assessment date and "START OF NEW ANTI-CANCER THERAPY" otherwise. SRCDOM
is "ADRS" for "DISEASE PROGRESSION" and for censoring at the last tumour
assessment, "DS" for "DEATH", and "ADSL" for censoring at the start of
new anti-cancer therapy. SRCVAR is "DSSTDTC" when SRCDOM is "DS", "ADT"
when SRCDOM is "ADRS", and "NACTDT" otherwise. SRCSEQ is the sequence
number of the record ADT traces to: the ASEQ of the progression record
or of the last valid assessment record, or the DSSEQ of the earliest
death record, and it has no value when the record is censored at the
start of new anti-cancer therapy.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
