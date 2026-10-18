Following CDISC SDTM standards, use the provided ODM dataset to create an AE
dataset with one record per reported event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESEV, AESER

STUDYID is the ODM study OID as given, and USUBJID is the ODM subject key as
given. Items belonging to one event share the subject, visit, visit repeat,
form, and form repeat identifiers. Only AE forms contribute records, and an
occurrence without a reported term contributes no AE record. Each form
occurrence is its own event, so a form repeat number reused at a later visit,
or the same term reported in two occurrences, still gives separate records.
AETERM holds the free-text term exactly as reported. AESTDTC and AEENDTC come
from the same event occurrence, and AEENDTC has no value when that occurrence
has no end date. AESEV and AESER come from that occurrence's severity and
seriousness items. AESEQ numbers the subject's events from 1 in visit order
(screening, then baseline), then by visit repeat and form repeat.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
