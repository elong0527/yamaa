Following CDISC ADaM standards, use the provided ADLB_RAW dataset to create an
ADLB dataset with one record per laboratory measurement in ADLB_RAW.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL, AVISIT, AWTARGET, ADIST,
ANL01FL

AVISIT is "WEEK 2" when ADY is from 8 through 22. AVISIT has no value when
ADY has no value or falls outside that range. AWTARGET is 15 when AVISIT is
"WEEK 2", and has no value when AVISIT has no value. ADIST is the number of
days between ADY and AWTARGET, without direction, and has no value when
AVISIT has no value. Within a subject, parameter, and visit, the record with
the smallest ADIST stands for that subject and parameter in the visit. When
two such records are equally close to the target, the one with the later ADY
stands; when they also share the same ADY, the one with the lower LBSEQ
stands. ANL01FL is "Y" on the record that stands for its subject and
parameter in the visit, and has no value on every other record. Every record
of ADLB_RAW stays in the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
