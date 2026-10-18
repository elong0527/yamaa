Following CDISC SDTM standards, use the provided KIT_LIST and ODM datasets to
create an EX dataset with one record per administration.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC

DOMAIN is "EX". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. Only a form whose occurrence item
(IT.EC.ECOCCUR) is "Y" gives a record; a skipped dose gives none.
EXTRT is the treatment as collected, except that a record whose
collected form is "KITDOSE" has the treatment the kit list holds for
its kit number. A record whose collected form is "TABLET" has
EXDOSE as the number of tablets times the tablet strength. A record
whose collected form is "INFUSION" has EXDOSE as the collected
mg/kg dose times the body weight collected at the same visit. A
record whose collected form is "KITDOSE" has EXDOSE as the dose
the kit list holds for its kit number, and a kit whose listed dose
is zero keeps that zero. A record whose collected form is
"AUCDOSE" has EXDOSE as the AUC target as collected. EXDOSE is not
rounded. EXDOSU is "AUC" for a record whose collected form is
"AUCDOSE" and "mg" for every other record. EXSTDTC and EXENDTC are
the start and end dates collected on the form. EXSEQ numbers each
subject's records from 1 by start date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
