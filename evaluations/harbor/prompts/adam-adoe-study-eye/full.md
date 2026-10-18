Following CDISC ADaM standards, use the provided ADSL and OE datasets to create
an ADOE dataset with one record per collected ophthalmic examination
measurement.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, OESEQ, PARAMCD, OELAT, AVAL, AFEYE

AFEYE is the eye's role in the study, from the collected laterality and
the subject's assigned eye in ADSL. AFEYE is "Both Eyes" for a bilateral
measurement when the subject has an assigned eye. AFEYE is "Study Eye"
when the measured eye matches the subject's assigned eye. When the
subject's assigned eye is both eyes, a measurement of either eye is
"Study Eye" whenever the collected laterality is present. AFEYE is
"Fellow Eye" when the measured eye is the opposite eye to the subject's
assigned eye. AFEYE has no value when the subject has no assigned eye or
the collected laterality is missing. The output holds every collected OE
measurement, and a measurement collected without a value is kept with
AVAL having no value. The assigned eye belongs to the subject, so the
same eye is the study eye at every visit, and only the collected
laterality moves a record between roles.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adoe.csv.
