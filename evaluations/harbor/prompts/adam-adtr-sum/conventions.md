Following CDISC ADaM standards, use the provided TR, TRVISIT, and TU datasets
to create an ADTR dataset with one record per subject and scheduled tumor
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS, NTARGET,
ANL01FL

PARAMCD is "SDIAM" and PARAM is "Sum of Target Lesion Diameters (mm)" on
every record. ANL01FL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
