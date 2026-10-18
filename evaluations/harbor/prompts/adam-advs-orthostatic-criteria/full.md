Following CDISC ADaM standards, use the provided VS dataset to create an ADVS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL, AVALC

PARAMCD and PARAM name the change or the flag the record holds: SYSBPOC is
"Systolic Blood Pressure Change on Standing (mmHg)", DIABPOC is "Diastolic
Blood Pressure Change on Standing (mmHg)", HROC is "Heart Rate Change on
Standing (beats/min)", OHYPO is "Orthostatic Hypotension", and OTACHY is
"Orthostatic Tachycardia".

On a change record, AVAL is the test's reading after three minutes standing
minus its supine reading at the same visit, not rounded, so a fall is
negative. The reading after one minute standing is never used. No change
record is added when either reading is missing or was not taken. AVALC has
no value on a change record, and AVAL has no value on a flag record.

On an OHYPO record, AVALC is "Y" when systolic blood pressure falls by 20
mmHg or more or diastolic blood pressure falls by 10 mmHg or more, and
either fall is enough on its own even when the other change is missing.
AVALC is "N" when both changes are present and neither falls that far. A
visit has no OHYPO record when neither change meets its threshold but one
of the changes is missing.

On an OTACHY record, AVALC is "Y" when heart rate rises by 30 beats/min or
more and "N" when it rises less. A visit has no OTACHY record when the
heart rate change is missing. Each threshold includes its boundary, so a
fall of exactly 20 or 10 mmHg, or a rise of exactly 30 beats/min, meets
it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
