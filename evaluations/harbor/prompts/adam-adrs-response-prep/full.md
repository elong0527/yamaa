Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL datasets to
create an ADRS dataset with one record per collected overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ

BORCAT is the response category the record can support in the best overall
response decision, from its collected response: "CR" for complete response,
"PR" for partial response, "SD" for stable disease, "NON-CR/NON-PD" for
neither complete response nor progressive disease, "PD" for progressive
disease, and "NE" for not evaluable. Stable disease and neither complete
response nor progressive disease count only on or after day 42 after
randomization; one earlier, or one with no day relative to randomization,
falls back to not evaluable, so its BORCAT is "NE". An assessment on or
after the subject's start of new anti-cancer therapy in ADSL supports no
category, whatever response was collected, so its BORCAT has no value. Any
other collected response, including a missing one, supports no category,
so BORCAT has no value. BORPRI orders the supported categories: 1 for
"CR", 2 for "PR", 3 for "SD", 4 for "NON-CR/NON-PD", 5 for "PD", and 6
for "NE". BORPRI has no value when the record supports no category.
BORSEQ numbers the records of each study and subject that support a
category, in BORPRI order, then by analysis date, then by assessment
sequence, starting at 1. A record that supports no category takes no
number: BORSEQ has no value on it, and numbering passes over it without
consuming a number. The record numbered 1 supplies the study-subject's
best overall response and its supporting date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
