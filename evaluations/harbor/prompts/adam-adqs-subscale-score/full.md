Following CDISC ADaM standards, use the provided QS dataset to create an ADQS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL, AREASND

The item records are the physical functioning records; a record of the
emotional functioning scale is not in the output. On an item record,
PARAMCD is the item's test code, PF01 through PF04, and PARAM is its
test name. AVAL on an item record is the numeric result on the zero to
four answer scale, and has no value when the item was not answered.
Every visit with a PF01 item record, answered or not, also has a score
record, with PARAMCD "PFSCORE" and PARAM "Physical Functioning
Subscale Score". AVAL on the score record is the mean of the answered
items at that visit multiplied by 25, and has no value when fewer than
three of the four items were answered. AVAL is not rounded.
AREASND on an item record is the reason recorded for the item not being
done, and has no value when none was recorded. On a score record whose
AVAL has no value, AREASND is the reason recorded on the visit's items
when none of them was answered and they record one reason between them,
and "NOT CALCULABLE" otherwise; it has no value on a score record with a
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
