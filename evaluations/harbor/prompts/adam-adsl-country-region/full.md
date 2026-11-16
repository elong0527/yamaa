Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, COUNTRY, REGION1

COUNTRY is the country in DM, as collected. REGION1 is "North America"
when COUNTRY is "USA" or "CAN", "Europe" when COUNTRY is "DEU", "ESP",
"FRA", or "GBR", and "Rest of World" for any other country.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
