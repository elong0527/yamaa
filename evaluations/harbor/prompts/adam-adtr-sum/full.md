Following CDISC ADaM standards, use the provided TR, TRVISIT, and TU datasets
to create an ADTR dataset with one record per subject and scheduled tumor
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS, NTARGET,
ANL01FL

PARAMCD is "SDIAM" and PARAM is "Sum of Target Lesion Diameters (mm)" on
every record. AVAL is the sum of TRSTRESN over the target longest-diameter
records at the assessment, those with TRGRPID of "TARGET" and TRTESTCD of
"LDIAM". A record with a missing result contributes nothing to the sum, while
a zero result counts as measured; AVAL has no value when the assessment
measured no target lesion. NMEAS is the count of non-missing TRSTRESN values
among those records. NMEAS has no value when the assessment has no target
longest-diameter records, and is 0 when it has such records but none of them
carries a result. NTARGET is the count of the subject's lesion inventory
records with TUGRPID of "TARGET", the lesions selected as target lesions at
study entry, and is the same at every assessment of the subject. ANL01FL is
"Y" when NMEAS equals NTARGET, and has no value otherwise. A sum over an
incomplete set of target lesions is still reported.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
