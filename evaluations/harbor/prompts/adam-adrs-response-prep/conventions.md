Following CDISC ADaM standards, use the provided ADRS_RAW dataset to
create an ADRS dataset with one record per collected overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ

BORCAT is the response category: complete response as CR, partial response as
PR, stable disease as SD, neither complete response nor progressive disease as
NON-CR/NON-PD, progressive disease as PD, or not evaluable as NE.

BORPRI orders the categories as 1 (complete response), 2 (partial response), 3
(stable disease), 4 (neither-complete-nor-progressive disease), 5 (progressive
disease), 6 (not evaluable).

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
