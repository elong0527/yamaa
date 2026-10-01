Following CDISC ADaM standards, use the provided QS dataset to create
an ADQS dataset with one record per subject per visit per item, plus
one score record per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL

Carry each physical functioning item response into PARAMCD (PF01
through PF04), PARAM, and AVAL, keeping the numeric result on its
zero to four answer scale, empty when the item was not answered.
Records from any other scale leave no records.

Add one score record per visit that has a PF01 item record, answered or
not, with PARAMCD "PFSCORE" and PARAM
"Physical Functioning Subscale Score". Its value is the mean of the
answered items at that visit, reported on a zero to one hundred scale;
it has no value when fewer than three of the four items were answered.
A visit with too few answers still gets its score record, with no
value, while a visit with no records at all gets none.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
