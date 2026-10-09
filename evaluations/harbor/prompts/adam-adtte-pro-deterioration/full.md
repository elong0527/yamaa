Following CDISC ADaM standards, use the provided ADSL, QS, and DS datasets
to flag QS deteriorations (one record per assessment) and create an ADTTE
dataset for time to PRO deterioration, one record per subject.

The flagged QS dataset should contain the following columns in this order:
STUDYID, USUBJID, QSSEQ, AVISIT, ADT, AVAL, DETERFL. The ADTTE dataset should
contain the following columns in this order: STUDYID, USUBJID, PARAMCD, PARAM,
STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ.

Use PARAMCD "TTDGHS" and PARAM "Time to Deterioration in Global Health
Status". STARTDT is the randomization date, the origin from which the time
to deterioration is counted. QSSEQ is the input sequence number, as given.

The baseline score is the score from the earliest assessment. An assessment
is a deterioration when it is dated after the baseline assessment and its
score is at least 10 points below the baseline score; the baseline
assessment itself is never a deterioration. DETERFL is Y for a
deterioration and has no value otherwise.

The event is the first deterioration, or death when the subject dies
without a prior deterioration; a deterioration and a death on the same date
count as deterioration. ADT is the deterioration date for a deterioration
event and the death date for a death event.

Disease progression, study discontinuation, and withdrawal of consent are
censoring reasons, never events; only the earliest such record counts. A
censored subject's ADT is the latest assessment dated on or before the
censoring reason, or the randomization date when no assessment qualifies. A
deterioration dated the same day as the censoring reason still counts as
the event.

AVAL is the number of whole calendar months from STARTDT to ADT. CNSR is 0
for a deterioration or death and 1 otherwise. EVNTDESC is PRO DETERIORATION
for a deterioration, DEATH for a death, and CENSORED otherwise. CNSDTDSC
has no value for an event and holds the censoring reason for a censored
record.

Trace ADT with SRCDOM QS, SRCVAR ADT, and the assessment sequence for a
deterioration; with SRCDOM ADSL, SRCVAR DTHDT, and no sequence for a death;
with SRCDOM QS, SRCVAR ADT, and the assessment sequence when censoring at
an assessment; and with SRCDOM ADSL, SRCVAR RANDDT, and no sequence when
censoring at randomization.

Read the source datasets from /app/input and save the completed datasets as
/app/output/qs_flagged.csv and /app/output/adtte.csv.
