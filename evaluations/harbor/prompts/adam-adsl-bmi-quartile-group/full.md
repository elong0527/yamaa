Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, BMIBL, BMIBLQ1, BMIBLMED, BMIBLQ3, BMIBLGR1

- BMIBLQ1, BMIBLMED and BMIBLQ3 are the first quartile, the median and the
  third quartile of the BMIBL values of the subjects in the study that have
  one. They are the same on every record, also on the record of a subject
  with no BMIBL.
- Each follows R's default quantile definition (type 7): with the n values
  in ascending order, the quantile at proportion p is at position
  (n - 1) * p + 1, and a position between two values gives the value
  between them in proportion to its distance from each.
- BMIBLQ1, BMIBLMED and BMIBLQ3 are not rounded.
- BMIBLGR1 is "Quartile 1" when BMIBL is at or below BMIBLQ1, "Quartile 2"
  when it is above BMIBLQ1 and at or below BMIBLMED, "Quartile 3" when it is
  above BMIBLMED and at or below BMIBLQ3, and "Quartile 4" when it is above
  BMIBLQ3. BMIBLGR1 has no value when BMIBL has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
