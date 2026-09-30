Following CDISC SDTM standards, use the provided VS_RAW dataset to create
a VS dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

Records are grouped by test rather than kept in collection order: all
heights first, then all weights, then all temperatures, each in collection
order. VSTESTCD is the test code as collected: HEIGHT, WEIGHT, or TEMP.
VSTEST is the test name as collected: Height, Weight, or Temperature.

VSORRES is the collected result written back as text; reading it as a
number drops trailing zeros, but the value itself is never converted.
VSORRESU is the unit exactly as collected: cm, kg, C, LB, or F, and has no
value when no result was collected. VSSTRESN is the result as a number in
the test's standard unit: height passes through in cm, weight collected in
LB is expressed in kg, and temperature collected in F is expressed in C; a
result already in the standard unit passes through unchanged, and a result
never collected stays missing. VSSTRESC is the same standardized value
written as text, so it always agrees with the numeric result, and has no
value when no result was collected. VSSTRESU is the test's standard unit:
cm for height, kg for weight, C for temperature; it has no value when no
result was collected. A unit that does not belong to the test stops the
run instead of being carried through. VSSTAT is NOT DONE when no result
was collected, and has no value otherwise.

A record whose result was never collected carries VSSTAT NOT DONE, no
original unit, and no standardized value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
