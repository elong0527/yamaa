Following CDISC SDTM standards, use the provided LOG dataset to create
an EC dataset with one record per dosing-log day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND,
ECDOSE, ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ

ECSEQ numbers the subject's dosing days in date order. ECTRT is Study
Drug and ECMOOD is PERFORMED on every record. ECOCCUR is Y when the
subject took the day's dose and N when the dose was not taken. ECREASND
is the reason the dose was not taken, such as SUBJECT FORGOT DOSE, and
has no value when the dose was taken. ECDOSE is the tablets taken that
day, with no value on a day the dose was not taken. ECDOSU is tablet,
ECDOSFRM is TABLET, ECDOSFRQ is QD, and ECROUTE is ORAL. ECSTDTC and
ECENDTC are both the dosing day, since one record covers one day. ECADJ
is the adjustment reason the log records for that day, such as DOSE
REDUCED, and has no value when the day records none. A missed day keeps
the unit, form, frequency, and route with no dose and the missed reason.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ec.csv.
