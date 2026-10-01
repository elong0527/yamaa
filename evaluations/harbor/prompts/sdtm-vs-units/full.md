Following CDISC SDTM standards, use the provided VS_RAW dataset to create
a VS dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

VSSEQ is the sequence number as collected. VSTESTCD is the test code as
collected: HEIGHT, WEIGHT, or TEMP. VSTEST is the test name as collected:
Height, Weight, or Temperature.

VSORRES is the collected result read as a number and written back as
text, so trailing zeros after the decimal point are dropped and a whole
number has no decimal point; the value itself is never converted.
VSORRESU is the unit exactly as collected: cm, kg, C, LB, or F, and has no
value when no result was collected. VSSTRESN is the result as a number in
the test's standard unit, not rounded: height passes through in cm,
weight collected in LB is multiplied by 0.45359237 to give kg, and
temperature collected in F becomes (F - 32) * 5 / 9 in C; a result
already in the standard unit passes through unchanged, and a result never
collected has no value. VSSTRESC is the same standardized value written as
text with at most 15 significant digits and no trailing zeros, so a whole
number has no decimal point; it has no value when no result was
collected. VSSTRESU is the test's standard unit: cm for height, kg for
weight, C for temperature; it has no value when no result was collected.
VSSTAT is NOT DONE when no result was collected, and has no value
otherwise.

A record whose result was never collected carries VSSTAT NOT DONE, no
original unit, and no standardized value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
