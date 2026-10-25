Following CDISC ADaM standards, use the provided IS dataset to create an ADIS
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVAL

Every record has PARAMCD "NABLOG10", the base-10 logarithm of the
neutralizing antibody titer. The titer is ISSTRESN, and the assay's lower
limit of quantitation is ISLLOQ. AVISIT is the visit name exactly as VISIT
spells it. AVAL is the base-10 logarithm of the titer at full precision; it
is not rounded, and it has no value when the titer has no value.

A titer below the lower limit of quantitation, a titer of zero among them,
has no record: zero has no logarithm, and a titer under the limit is not a
measured value to transform. A titer equal to the limit has a record, and so
does a visit with no titer, whose AVAL has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adis.csv.
