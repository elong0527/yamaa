Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per reported result from the serum,
skin-biopsy, saliva, and tape-strip forms.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT,
LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT,
LBDTC

VISIT is SCREENING, BASELINE, DAY 21, or UNSCHEDULED for the SCRN, BL, D21,
and UNSCH study events. VISITNUM is 1, 2, and 3 for the planned visits, and
2.01 and 2.02 for the first and second unscheduled visit after baseline.
LBTESTCD is VITD25OH, IL13, or CAMPPRO; LBTEST is 25-Hydroxyvitamin D,
Interleukin 13 mRNA, or Cathelicidin Protein; LBCAT is CHEMISTRY, GENE
EXPRESSION, or ANTIMICROBIAL PEPTIDE. LBSPEC is SERUM, SKIN BIOPSY, SALIVA,
or TAPE STRIP. LBLOC is LESIONAL or NON-LESIONAL, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
