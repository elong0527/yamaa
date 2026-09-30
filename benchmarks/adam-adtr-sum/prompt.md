Following CDISC ADaM standards, use the provided TRVISIT, TR, and TU
datasets to create an ADTR dataset with one record per subject per
scheduled tumor assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS,
NTARGET, ANL01FL

Use PARAMCD "SDIAM" and PARAM "Sum of Target Lesion Diameters (mm)" on
every record.

AVAL is the sum of the target longest-diameter records at the
assessment: records with lesion group TARGET and test code LDIAM. A
record with no result contributes nothing, while a zero result counts
as measured; AVAL has no value when the assessment measured no target
lesion.

NMEAS is how many target lesions the assessment measured: the count of
records with a result. It has no value when the assessment has no
target longest-diameter records, and is 0 when it has records but none
carries a result. NTARGET is how many lesions were selected as target
lesions at study entry, and is the same at every assessment of the
subject.

ANL01FL is Y when every target lesion was measured, that is when NMEAS
equals NTARGET, and has no value otherwise. Lesions outside the target
inventory never enter the sum.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
