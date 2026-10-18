Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COUNTRY, REGION1

COUNTRY is the collected country in upper case, or "UNKNOWN". REGION1 is
"North America" for "USA" or "CAN", "Europe" for "DEU", and "Rest of
World" for any other country.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
