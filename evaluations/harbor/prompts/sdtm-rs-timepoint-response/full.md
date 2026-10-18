Following CDISC SDTM standards, use the provided ODM dataset to create an RS
dataset with one record per subject and scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, RSSEQ, RSTESTCD, RSTEST, RSSTRESC,
RSSTAT

STUDYID is the ODM study OID and USUBJID is the ODM subject key, each as
given. AVISIT is the name of the scheduled assessment, AVISITN is its
numeric order, and ADT is its date. RSSEQ numbers a subject's records in
visit order, starting at 1. RSTESTCD is "TRGRESP" and RSTEST is "Timepoint
Response" on every record. RSSTAT is "NOT DONE" for a scheduled assessment
with no tumor measurement records at all, and has no value otherwise.
RSSTRESC has no value when the assessment was not done. A baseline
assessment is never compared against itself, so its response is "NE"
whenever it was done. RSSTRESC is "NE" when fewer target lesions were
measured at the assessment than were chosen at study entry. An assessment
at which some but not all of the chosen target lesions were measured is
still done, and its response is "NE". RSSTRESC is "NE" when the subject has
non-target disease only. RSSTRESC is "NE" when no percent change from
baseline can be computed, for example when the baseline target sum is zero
or missing. The target sum at an assessment is the sum of the longest
diameters of the target lesions measured at that assessment. RSSTRESC is
"CR" when every target lesion has disappeared. RSSTRESC is "PR" when the
target sum shrank at least 30% from baseline. RSSTRESC is "PD" when the
target sum grew at least 20% from baseline. RSSTRESC is "SD" otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/rs.csv.
