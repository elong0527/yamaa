Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per reported result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT,
LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT, LBDTC

DOMAIN is "LB" on every record. STUDYID is the study OID as given and
USUBJID the subject key as given. VISIT is "SCREENING" for the screening
visit, "BASELINE" for the baseline visit, "DAY 21" for the day 21 visit,
and "UNSCHEDULED" for an unscheduled visit. VISITNUM is 1 for the screening
visit, 2 for the baseline visit, and 3 for the day 21 visit; an unscheduled
visit is numbered after the baseline visit by its occurrence at that visit,
2.01 for the first occurrence and 2.02 for the second, so repeated
unscheduled visits stay distinct. A serum result has LBTESTCD "VITD25OH",
LBTEST "25-Hydroxyvitamin D", LBCAT "CHEMISTRY", and LBSPEC "SERUM". A skin
biopsy result has LBTESTCD "IL13", LBTEST "Interleukin 13 mRNA", LBCAT
"GENE EXPRESSION", and LBSPEC "SKIN BIOPSY". A saliva result has LBTESTCD
"CAMPPRO", LBTEST "Cathelicidin Protein", LBCAT "ANTIMICROBIAL PEPTIDE", and
LBSPEC "SALIVA". A tape-strip result has LBTESTCD "CAMPPRO", LBTEST
"Cathelicidin Protein", LBCAT "ANTIMICROBIAL PEPTIDE", and LBSPEC
"TAPE STRIP". LBLOC is "LESIONAL" on a lesional biopsy or tape-strip result
and "NON-LESIONAL" on a non-lesional one, and has no value on a serum or
saliva result. LBORRES is the collected result exactly as reported, and
LBSTRESC holds the same value. LBORRESU is "CYCLE" on a skin biopsy record
and "ng/mL" on every other record, and LBSTRESU is the same unit as
LBORRESU. LBSTRESN is the numeric form of the reported result. An item with
no reported value gives no record, while a collected zero is kept as a real
result. An entry of "NOT DONE" still gives a record: LBSTAT is "NOT DONE",
and LBORRES, LBORRESU, LBSTRESC, LBSTRESN, and LBSTRESU have no value on it.
LBSTAT has no value on every other record. LBDTC is the collection date
collected on the same form as the result, so a form collected twice at one
visit keeps each occurrence's own date. LBSEQ numbers each subject's
records from 1 by collection date, then test code, then specimen type.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
