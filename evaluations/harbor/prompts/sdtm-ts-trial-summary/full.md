Following CDISC SDTM standards, use the provided PROTOCOL dataset to create a
TS dataset with one record per protocol parameter per study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, TSSEQ, TSPARMCD, TSPARM, TSVAL, TSVALCD, TSVCDREF

DOMAIN is "TS", and STUDYID is the protocol Study identifier.
For title, TSSEQ is 1, TSPARMCD is "TITLE", TSPARM is "Trial Title", and TSVAL
is the protocol Title.
For phase, TSSEQ is 2, TSPARMCD is "TPHASE", and TSPARM is "Trial Phase
Classification"; Phase "Phase 2" has TSVAL "PHASE II TRIAL" and TSVALCD
"C15601".
For randomization, TSSEQ is 3, TSPARMCD is "RANDOM", and TSPARM is "Trial is
Randomized"; Randomized "Yes" has TSVAL "Y" and TSVALCD "C49488".
For planned enrollment, TSSEQ is 4, TSPARMCD is "PLANSUB", and TSPARM is
"Planned Number of Subjects"; TSVAL is PlannedSubjects as integer text with no
decimal point.
TSVCDREF is "CDISC" for phase and randomization.
For title and planned enrollment, TSVALCD has no value, and TSVCDREF has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ts.csv.
