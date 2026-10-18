Following CDISC SDTM standards, use the provided ODM and VS_TESTS datasets to
create a VS dataset with one record per test from the vital signs form
collected at each visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSPOS, VSMETHOD, VSSTAT, VSREASND, VSDTC

DOMAIN is "VS". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. VISIT is the name of the visit at which the
form was collected, as given in ODM. VSDTC is the visit date collected
on the form. Each visit's form group fans out into one record per test.
VSTESTCD is "SYSBP", "DIABP", "PULSE", "RESP", "TEMP", or "WEIGHT".
VSTEST is the test name VS_TESTS gives for the test code: "Systolic
Blood Pressure", "Diastolic Blood Pressure", "Pulse Rate", "Respiratory
Rate", "Temperature", or "Weight". VSORRES is the result exactly as
collected on the form, and has no value when the measurement was not
done. VSORRESU is the unit exactly as collected: "mmHg" for the blood
pressure tests, "beats/min" for pulse, "breaths/min" for respiratory
rate, "C" for temperature, and "kg" for weight; it has no value when no
result was collected. VSSTRESN is the numeric result in the standard
unit; the form already collects standard units, so it equals the
collected result, and it has no value when no result was collected.
VSSTRESC is the same standardized value written as text, so it always
agrees with the numeric result, and it has no value when no result was
collected. VSSTRESU is the standard unit, equal to the collected unit,
and it has no value when no result was collected. VSPOS is the body
position as collected on the form on blood pressure and pulse records
with a collected result, and has no value otherwise. VSMETHOD is the
measurement method as collected on the form on temperature records with
a collected result, and has no value otherwise. VSSTAT is "NOT DONE" on
the blood pressure records when the form flags blood pressure as not
done, and has no value otherwise. VSREASND is the reason the form gives
for blood pressure not done, on the blood pressure records, and has no
value otherwise. A not-done blood pressure still gives its two records,
so the reason is kept, but those records carry no result, unit,
position, or standardized value. VSSEQ numbers each subject's records
from 1 by visit date, then in test order within each visit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
