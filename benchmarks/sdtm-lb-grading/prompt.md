Following CDISC SDTM standards, use the provided LB_RAW dataset to create
an LB dataset with one laboratory record per collected neutrophil or
hemoglobin result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBTOXGR

LBSEQ is the sequence number as collected. LBTESTCD is the test short
name as collected: ANC for absolute neutrophil count or HGB for
hemoglobin. LBSTRESN is the standardized numeric result as collected.
LBTOXGR is the standard toxicity grade for the result: 4, 3, 2, 1, or 0.
Each band includes its lower limit and excludes its upper one.

Neutrophil counts use one band set whatever the sex, with the example
lab's lower limit of normal at 1.8: below 0.5 is grade 4, 0.5 to below 1.0
is grade 3, 1.0 to below 1.5 is grade 2, 1.5 to below 1.8 is grade 1, and
1.8 or above is grade 0. Hemoglobin uses one band set per sex: below 8.0
is grade 3, below 10.0 is grade 2, and below 13.5 for males (below 12.0
for females) is grade 1; higher results are grade 0.

A result for any other test, or a hemoglobin result with a sex other than
M or F, gets no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
