Following CDISC SDTM standards, use the provided VS_RAW dataset to create a VS
dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

DOMAIN is "VS". VSTESTCD is the test code as collected: "HEIGHT",
"WEIGHT", or "TEMP". VSTEST is the test name as collected: "Height",
"Weight", or "Temperature". VSORRES is the collected result written
back as text; read as a number it drops trailing zeros (a collected
72.50 reads 72.5). VSSTAT is "NOT DONE" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
