Following CDISC ADaM standards, use the provided ADTR_RAW dataset to
create an ADTR dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS,
NTARGET, ANL01FL, NADIR

Use PARAMCD "SDIAM" and PARAM "Sum of Target Lesion Diameters (mm)" on
every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
