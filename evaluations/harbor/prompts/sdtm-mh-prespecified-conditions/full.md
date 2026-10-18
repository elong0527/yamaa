Following CDISC SDTM standards, use the provided MH_ITEMS and ODM datasets
to create an MH dataset with one record per condition asked about on the
checklist or volunteered in free text.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, MHSEQ, MHTERM, MHCAT, MHPRESP, MHOCCUR, MHSTAT

DOMAIN is "MH". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. Each checklist condition asked of a subject
gives one record, answered or not, and a checklist condition that does
not appear for the subject gives no record. MHTERM is the condition the
item-definition table gives for a checklist item. For a volunteered
condition, MHTERM is the volunteered text exactly as reported. MHCAT is
"DISEASE-SPECIFIC HISTORY" for a checklist record and "GENERAL HISTORY"
for a volunteered record. MHPRESP is "Y" for a checklist record and has
no value for a volunteered record. MHOCCUR is "Y" or "N" for an answered
checklist condition, as answered, and has no value otherwise. MHSTAT is
"NOT DONE" when a checklist condition was not answered and has no value
otherwise. Each volunteered condition is its own record, even when its
form repeat number is used again at another visit. MHSEQ numbers each
subject's records from 1: the checklist conditions first, in the order
they are shown on the form, then the volunteered conditions by visit,
screening before baseline, and then by form repeat number.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
