Following CDISC SDTM standards, use the provided ODM dataset to create an FA
dataset with one record per collected reaction finding.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTAT, FATPT, FADTC

DOMAIN is "FA". FATESTCD is "OCCUR", "SEV", or "LDIAM", and FATEST is
"Occurrence Indicator", "Severity/Intensity", and "Longest Diameter".
FACAT is "REACTOGENICITY" for every record. FASTAT is "NOT DONE" or has
no value. FATPT is "END DAY 1" for diary day DAY1 and "END DAY 2" for
diary day DAY2.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
