Following CDISC SDTM standards, use the provided DM_RAW dataset to
create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, COUNTRY

USUBJID is the study, site, and subject identifiers with hyphens
between them. SITEID is the collected site identifier, and SUBJID and
COUNTRY are as collected. AGE is the collected age in whole years, with
no value when not collected. AGEU is fixed to YEARS. SEX is the
collected sex, one of F, M, or U.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
