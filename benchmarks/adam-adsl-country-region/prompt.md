Following CDISC ADaM standards, use the provided DM dataset to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COUNTRY, REGION1

COUNTRY is the collected country in upper case; a subject with no
collected country is UNKNOWN. Every collected country passes through:
there is no allowed-countries check, so the run never stops on a new
country. Case differences never split a country.

REGION1 is the region for that country: USA and CAN give North
America, DEU gives Europe, and any other country (including UNKNOWN)
gives Rest of World. Every subject has both a country and a region.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
