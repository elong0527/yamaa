Following CDISC ADaM standards, use the provided QS dataset to create an ADQS
dataset with one record per subject, visit, and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

PARAMCD on an item record is the item's test code and PARAM is its test
name. AVAL on an item record is the item's numeric result on its answer
scale, and has no value when the item was not answered. The physical
functioning scale holds items F101, F102, F103, and F104, and its score
record has PARAMCD "F1SCORE" and PARAM "Physical Functioning Scale Score".
The role functioning scale holds items F201 and F202, and its score record
has PARAMCD "F2SCORE" and PARAM "Role Functioning Scale Score". The fatigue
and sleep symptom scale holds items S01, S02, and F104, and its score
record has PARAMCD "SSCORE" and PARAM "Fatigue and Sleep Symptom Scale
Score". The global health scale holds items G01 and G02, and its score
record has PARAMCD "GSCORE" and PARAM "Global Health Scale Score". AVAL on
a score record is the mean of the scale's answered item responses at that
visit, rescaled linearly onto a zero to one hundred scale, and is not
rounded. On the physical functioning and role functioning scales a mean
response of 1 reads 100 and a mean response of 4 reads 0, so a higher score
reads as better functioning. On the fatigue and sleep symptom scale a mean
response of 1 reads 0 and a mean response of 4 reads 100, so a higher score
reads as worse symptoms. On the global health scale a mean response of 1
reads 0 and a mean response of 7 reads 100, so a higher score reads as
better health. AVAL on a physical functioning or fatigue and sleep symptom
score record has no value when fewer than two of the scale's items were
answered at that visit. AVAL on a role functioning or global health score
record has no value when none of the scale's items was answered at that
visit. A scale's score record is added at a visit only when the visit
holds that scale's anchor item record, F101 for physical functioning, F201
for role functioning, S01 for fatigue and sleep symptoms, and G01 for
global health, and it is added whether or not the anchor item was
answered, so a visit with too few answers keeps its score record with no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
