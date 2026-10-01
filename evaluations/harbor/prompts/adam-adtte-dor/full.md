Following CDISC ADaM standards, use the provided ADSL, ADRS_RAW, and DS
datasets to create an ADTTE dataset for duration of response, with one
record per responder. A responder is a subject with a response date
(RESPDT) in ADSL.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "DOR" and PARAM "Duration of Response". STARTDT is the
response date.

The event is disease progression (the earliest assessment with a PD
response) or death (the earliest DS record with a DEATH term), whichever
comes first. An event counts only when it falls on or before the start of
new anti-cancer therapy (NACTDT in ADSL).

A responder without a counted event is censored at the earlier of the last
evaluable tumour assessment and the start of new anti-cancer therapy. An
evaluable assessment has a date and a response other than NE, whether it
falls before or after the start of new therapy.

ADT is the event or censoring date. AVAL is the number of days from
STARTDT to ADT, counting both days. CNSR is 0 for an event and 1 for a
censored record.

Describe each record with these values:
- EVNTDESC is DISEASE PROGRESSION, DEATH, or CENSORED. When progression
  and death fall on the same day, report DISEASE PROGRESSION.
- CNSDTDSC is LAST TUMOUR ASSESSMENT or START OF NEW ANTI-CANCER THERAPY
  for a censored record, and has no value for an event.

Identify the source of ADT with SRCDOM, SRCVAR, and SRCSEQ:
- progression, or censoring at the last tumour assessment: ADRS, ADT, and
  the assessment's ASEQ;
- death: DS, DSSTDTC, and DSSEQ;
- censoring at the start of new therapy: ADSL, NACTDT, and no sequence.

When several records share the selected date, use the lowest sequence
number for progression or death and the highest for the last tumour
assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
