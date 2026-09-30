Following CDISC SDTM standards, use the provided ODM, DM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

VSSEQ numbers the records per subject from 1 in schedule order, then
pulse, systolic, and diastolic blood pressure within each time point.
VISIT is DAY 1 for study event SE.D1. VSTPT is the planned time point name
for the collection form: PRE-DOSE, 30 MIN POST-DOSE, 1 H POST-DOSE, or
4 H POST-DOSE. VSTPTNUM is the planned time point number for the form: 1
through 4 in schedule order. VSELTM is the planned elapsed time since the
first dose for the form: -PT15M before the dose, then PT30M, PT1H, and
PT4H after it.

VSTESTCD is the test short name from the test dictionary: PULSE, SYSBP, or
DIABP. Items with no dictionary entry give no record. VSORRES is the
result in original units, and VSORRESU the original unit from the test
dictionary (beats/min or mmHg). VSSTRESN is the numeric result in standard
units; pulse and blood pressure were collected in standard units, so it
equals the original result. VSSTRESU is the standard unit, carried from
the original unit. VSDTC is the actual collection datetime from the form's
datetime item row, shared by every result on that form. VSDY is the study
day of the collection datetime, counted from the subject's reference start
date: that date is day 1, there is no day zero, and dates before it count
back from -1; it has no value when the subject has no reference start
date.

The planned elapsed time stays next to the actual collection datetime, so
a reading taken later than planned keeps its late actual datetime.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
