Following CDISC SDTM standards, use the provided ODM and KITLIST
datasets to create an EX dataset with one record per administered
exposure.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC

Only a form whose occurrence is Y gives a record, so a skipped dose
leaves none; the placebo kit keeps its zero dose. EXSEQ numbers the
administrations within the subject by start date and then by the form
repeat number within the visit. EXTRT is the kit list treatment for a
blinded kit, and the collected treatment otherwise. EXDOSE is the
administered dose: for tablets, the tablets taken at the collected
strength; for infusion, the collected amount per kilogram at the visit
body weight; for a blinded kit, the kit list dose; and for an AUC
target, the collected target. EXDOSU is mg, except the AUC target keeps
AUC. EXSTDTC and EXENDTC are the collected start and end dates. Body
weight comes from the VS form at the same visit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
