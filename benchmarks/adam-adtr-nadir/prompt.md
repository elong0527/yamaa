Following CDISC ADaM standards, use the provided ADTR_RAW dataset to
create an ADTR dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, AVISITN, ADT, PARAMCD, PARAM, AVAL, NMEAS,
NTARGET, ANL01FL, NADIR

Use PARAMCD "SDIAM" and PARAM "Sum of Target Lesion Diameters (mm)" on
every record.

NADIR is the lowest AVAL among complete assessments (ANL01FL is Y) for
the same subject dated on or before the current ADT, including the
current assessment itself. It has no value when the current ADT has
none, or when no complete assessment falls in the window, so an
incomplete current assessment keeps the nadir set by an earlier
complete one, while a complete assessment with no date never counts
toward any nadir, its own included.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtr.csv.
