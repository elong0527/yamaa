Following CDISC ADaM standards, use the provided ADSL_RAW and DS datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, COMPLFL

COMPLFL is "Y" when the subject has a disposition record with category
"DISPOSITION EVENT" in the "FOLLOW-UP" epoch whose standardized outcome
is "COMPLETED", and "N" otherwise. The completion record counts whether
or not its collection date was recorded. A "COMPLETED" outcome in an
earlier epoch does not count, so a subject who completed treatment but
discontinued the study has COMPLFL "N". A subject with no disposition
record at all, or whose disposition records carry only other outcomes
such as "ADVERSE EVENT", has COMPLFL "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
