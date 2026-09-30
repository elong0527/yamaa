Following CDISC ADaM standards, use the provided OE and ADSL datasets
to create an ADOE dataset with one row per collected ophthalmic
measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE

AFEYE is the eye's role in the study: Study Eye when the measured eye
matches the subject's assigned eye, Both Eyes for a bilateral
measurement when the subject has an assigned eye, and Fellow Eye for
the opposite eye. Either eye counts as the study eye when both eyes
are assigned and the collected laterality is present. When the
assigned eye or the collected laterality is missing, AFEYE has no
value.

The assigned eye belongs to the subject, so the same eye is the study
eye at every visit; only the collected laterality moves a record
between roles. A measurement collected without a value is kept with
its value empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adoe.csv.
