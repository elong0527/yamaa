Following CDISC ADaM standards, use the provided QS dataset to create
an ADQS dataset with one record per subject per visit per item, plus
one score record per scale per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

The physical functioning items are PARAMCD F101 through F104, the role
functioning items F201 and F202, the fatigue and sleep symptom items S01,
S02, and F104, and the global health items G01 and G02. Score records have
PARAMCD "F1SCORE", "F2SCORE", "SSCORE", and "GSCORE" with PARAM "Physical
Functioning Scale Score", "Role Functioning Scale Score", "Fatigue and Sleep
Symptom Scale Score", and "Global Health Scale Score". Item responses are on
a one to four answer scale, except the global health items on a one to seven
scale. Scale scores are reported on a zero to one hundred scale: higher for
better functioning on the functioning scales, higher for worse symptoms on
the symptom scale, and higher for better health on the global scale.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
