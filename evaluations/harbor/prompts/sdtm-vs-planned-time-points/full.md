Following CDISC SDTM standards, use the provided DM, ODM, and VS_MAPPING
datasets to create a VS dataset with one record per collected vital-signs
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTPT, VSTPTNUM, VSELTM, VSTESTCD,
VSORRES, VSORRESU, VSSTRESN, VSSTRESU, VSDTC, VSDY

DOMAIN is "VS". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. VISIT is "DAY 1" on every record. Each planned
time point is collected on its own vital-signs form, and the form fixes
the planned time point of every result on it: VSTPT is "PRE-DOSE",
"30 MIN POST-DOSE", "1 H POST-DOSE", or "4 H POST-DOSE" for the form.
VSTPTNUM is the planned time point number for the form: 1, 2, 3, or 4 in
schedule order. VSELTM is the planned elapsed time since the first dose
for the form, in ISO 8601 duration form: "-PT15M" before the dose, then
"PT30M", "PT1H", and "PT4H" after it. VSTESTCD is the test short name the
test dictionary gives for the result's item: "PULSE", "SYSBP", or
"DIABP". An item with no dictionary entry, such as the collection
datetime item or a dosing item, gives no record. VSORRES is the result in
original units, the value on the test's item row within the form.
VSORRESU is the original unit from the test dictionary: "beats/min" for
pulse and "mmHg" for blood pressure. VSSTRESN is the numeric result in
standard units; pulse and blood pressure were collected in standard
units, so it equals the original result. VSSTRESU is the standard unit,
the same as the original unit. VSDTC is the actual collection datetime
from the form's datetime item row, the same for every result on that
form. VSDY is the study day of the collection datetime, counted from the
subject's reference start date in DM, and has no value when the subject
has no reference start date. VSSEQ numbers each subject's records from 1
in schedule order by planned time point, then pulse, systolic blood
pressure, and diastolic blood pressure within each time point. The
planned elapsed time stays next to the actual collection datetime, so a
reading taken later than planned keeps its late actual datetime.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
