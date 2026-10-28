Following CDISC SDTM standards, use the provided PROTOCOL dataset to create a
TS dataset with one record per protocol parameter per study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, TSSEQ, TSPARMCD, TSPARM, TSVAL, TSVALCD, TSVCDREF

DOMAIN is "TS". TSPARMCD and TSPARM are "TITLE" and "Trial Title", "TPHASE"
and "Trial Phase Classification", "RANDOM" and "Trial is Randomized", or
"PLANSUB" and "Planned Number of Subjects". TSVAL "PHASE II TRIAL" has
TSVALCD "C15601"; TSVAL "Y" has TSVALCD "C49488". TSVALCD has no value as
well. TSVCDREF is "CDISC" or has no value. For planned enrollment, TSVAL is
integer text with no decimal point.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ts.csv.
