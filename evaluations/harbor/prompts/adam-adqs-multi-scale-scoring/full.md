Following CDISC ADaM standards, use the provided QS dataset to create
an ADQS dataset with one record per subject per visit per item, plus
one score record per scale per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

Carry each item response into PARAMCD (its test code), PARAM (its test
name), and AVAL, keeping the numeric result on its answer scale, empty
when the item was not answered.

The instrument has four scales. The physical functioning scale holds
items F101 through F104, with score PARAMCD "F1SCORE" and PARAM
"Physical Functioning Scale Score". The role functioning scale holds
items F201 and F202, with score PARAMCD "F2SCORE" and PARAM "Role
Functioning Scale Score". The fatigue and sleep symptom scale holds
items S01, S02, and F104, with score PARAMCD "SSCORE" and PARAM
"Fatigue and Sleep Symptom Scale Score"; item F104 also belongs to the
physical functioning scale and feeds both scores. The global health
scale holds items G01 and G02, with score PARAMCD "GSCORE" and PARAM
"Global Health Scale Score".

A scale score is the mean of the scale's answered items at that visit,
reported on a zero to one hundred scale. On the functioning scales a
higher score means better functioning: the score is 100 times one minus
the raw mean minus one over the range. On the symptom and global scales
a higher score means worse symptoms or better health: the score is 100
times the raw mean minus one over the range. The range is 3 for items
answered one to four and 6 for items answered one to seven.

A score needs at least two answered items for the physical functioning
and symptom scales and at least one for the role functioning and global
scales; with too few answered items the score has no value. Add one
score record per scale for each visit that has the scale's anchor item
record (F101, F201, S01, or G01), answered or not. A visit with too few
answers still gets its score record, with no value, while a visit with
no records at all gets none.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
