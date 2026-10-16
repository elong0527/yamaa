Following CDISC ADaM standards, use the provided VS dataset to create
an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL, AVALC

The parameters are:
- SYSBPOC, "Systolic Blood Pressure Change on Standing (mmHg)"
- DIABPOC, "Diastolic Blood Pressure Change on Standing (mmHg)"
- HROC, "Heart Rate Change on Standing (beats/min)"
- OHYPO, "Orthostatic Hypotension"
- OTACHY, "Orthostatic Tachycardia"

On a change record, AVAL is the test's reading after three minutes
standing minus its supine reading at the same visit: systolic blood
pressure for SYSBPOC, diastolic blood pressure for DIABPOC, and heart
rate for HROC. AVAL is not rounded, so a fall is negative. The reading
after one minute standing is never used. No change record is added
when either reading is missing or was not taken, and AVALC has no
value on a change record.

On a flag record, AVAL has no value. On an OHYPO record, AVALC is "Y"
when systolic pressure falls by 20 mmHg or more or diastolic pressure
falls by 10 mmHg or more, either fall being enough even when the
other change is missing; AVALC is "N" when both changes are present
and neither falls that far. A visit gets no OHYPO record when neither
change meets its threshold but one of them is missing. On an OTACHY
record, AVALC is "Y" when heart rate rises by 30 beats/min or more
and "N" when it rises less; a visit gets no OTACHY record when the
heart rate change is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
