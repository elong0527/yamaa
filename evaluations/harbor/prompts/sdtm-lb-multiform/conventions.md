Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per reported result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT,
LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT, LBDTC

DOMAIN is "LB" on every record. VISIT is "SCREENING", "BASELINE", "DAY 21",
or "UNSCHEDULED". VISITNUM is 1 for the screening visit, 2 for the baseline
visit, and 3 for the day 21 visit. A serum result has LBTESTCD "VITD25OH",
LBTEST "25-Hydroxyvitamin D", LBCAT "CHEMISTRY", and LBSPEC "SERUM". A skin
biopsy result has LBTESTCD "IL13", LBTEST "Interleukin 13 mRNA", LBCAT "GENE
EXPRESSION", and LBSPEC "SKIN BIOPSY". A saliva result has LBTESTCD
"CAMPPRO", LBTEST "Cathelicidin Protein", LBCAT "ANTIMICROBIAL PEPTIDE", and
LBSPEC "SALIVA", and a tape-strip result has the same values with LBSPEC
"TAPE STRIP". LBLOC is "LESIONAL" or "NON-LESIONAL", or has no value. LBSTAT
is "NOT DONE" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
