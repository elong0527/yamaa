Following CDISC SDTM standards, use the provided ODM dataset to create an
RS dataset with one record per subject and scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST, RSSTRESC,
RSSTAT

AVISITN is the numeric order of the assessment, ADT its date, and RSSEQ
numbers a subject's response records in visit order, starting at 1.
RSTESTCD is TRGRESP and RSTEST is Timepoint Response on every record.

RSSTRESC is NE at baseline, when fewer target lesions were measured than
were chosen, when the subject has non-target disease only, or when no
percent change can be computed (for example a zero or missing baseline
sum); CR when every target lesion has disappeared; PR when the target sum
shrank at least 30% from baseline; PD when it grew at least 20% from
baseline; and SD otherwise. It has no value when the assessment was not
done. RSSTAT is NOT DONE for a scheduled assessment with no tumor
measurement records at all, and has no value otherwise.

A baseline assessment is never compared against itself, so its response is
NE whenever it was done. An assessment with some but not all chosen target
lesions measured is still done, and its response is NE.

Read the source datasets from /app/input and save the completed dataset as
/app/output/rs.csv.
