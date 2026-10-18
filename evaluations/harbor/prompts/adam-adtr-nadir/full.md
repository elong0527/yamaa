Following CDISC ADaM standards, use the provided ADTR_RAW dataset to create an
ADTR dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS, NTARGET,
ANL01FL, NADIR

Every record has PARAMCD "SDIAM" and PARAM "Sum of Target Lesion
Diameters (mm)". NADIR is the lowest AVAL among the subject's complete
assessments, those with ANL01FL "Y", dated on or before the record's
ADT, including the record itself when it is complete. NADIR has no
value when the record's ADT has no value, or when no complete
assessment is dated on or before it. An incomplete record keeps the
NADIR set by an earlier complete assessment, and its own AVAL does not
count toward it. A complete assessment whose ADT has no value never
counts toward any NADIR, its own included.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
