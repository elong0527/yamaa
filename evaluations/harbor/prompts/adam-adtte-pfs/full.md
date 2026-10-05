Following CDISC ADaM standards, use the provided ADSL, RS, and DS
datasets to create an ADTTE dataset for progression-free survival,
with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "PFS" and PARAM "Progression-Free Survival". STARTDT is
the randomization date. ADSL also carries NTXSTDT, the start date of
new anti-cancer therapy, for subjects who began one.

The event is disease progression (the first adequate overall-response
assessment with a PD result) or death (the first disposition record
with a DEATH outcome), whichever comes first. Only an assessment
flagged adequate can supply a progression event or a censoring date,
and an assessment without a date never counts. Records sharing a date
are ordered by their sequence number.

When new anti-cancer therapy started, only dates on or before the
therapy start are usable: a progression or death dated after therapy
start is not an event, and the subject is censored at the last adequate
assessment dated on or before therapy start.

A subject with an event has ADT at the earlier of the progression and
death dates; a subject without one is censored at the last adequate
assessment date, or at the randomization date when no adequate
assessment can supply a censoring date. The event always wins, even when
it comes after the last adequate assessment, unless new anti-cancer
therapy started first.

AVAL is the number of days from STARTDT to ADT, counting the
randomization day as day one. CNSR is 0 for an event and 1 for a
censored record.

Describe each record with EVNTDESC DISEASE PROGRESSION, DEATH, or
CENSORED. A progression and a death on the same day count as
progression.

Trace ADT with SRCDOM DS, SRCVAR DSDTC, and the disposition sequence
for a death; otherwise with SRCDOM RS, SRCVAR RSDTC, and the response
sequence of the progression event or the censored assessment. Leave
SRCDOM, SRCVAR, and SRCSEQ empty when the subject is censored at
randomization.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
