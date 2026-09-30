Following CDISC SDTM standards, use the provided ODM and TESTS datasets to
create a VS dataset with one record per test from the wide vital signs
form collected at each visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC

VSSEQ numbers each subject's records by visit date, then in test order
within each visit. VISIT is the visit name and VSDTC the measurement date.
VSTESTCD is SYSBP, DIABP, PULSE, RESP, TEMP, or WEIGHT; VSTEST is Systolic
Blood Pressure, Diastolic Blood Pressure, Pulse Rate, Respiratory Rate,
Temperature, or Weight.

VSORRES is the result exactly as collected on the form, and has no value
when the measurement was not done. VSORRESU is the unit exactly as
collected (mmHg, beats/min, breaths/min, C, or kg), and has no value when
no result was collected. VSSTRESN is the numeric result in the standard
unit; the form already collects standard units, so it equals the collected
result, and has no value when no result was collected. VSSTRESC is the
same standardized value written as text, so it always agrees with the
numeric result; a whole number is written without a decimal point.
VSSTRESU is the standard unit, equal to the collected unit, and has no
value when no result was collected. VSPOS is the body position as
collected (SITTING) on blood pressure and pulse records with a collected
result, and has no value otherwise. VSMETHOD is the measurement method as
collected (ORAL) on temperature records with a collected result, and has
no value otherwise. VSSTAT is NOT DONE when the form flags blood pressure
as not done, and has no value otherwise; VSREASND is the reason the form
gives, and has no value otherwise.

A not-done blood pressure still produces its two records so the reason is
kept, but they carry no result, unit, position, or standardized value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
