Following CDISC SDTM standards, use the provided LBRANGE and ODM datasets to
create an LB dataset with one record per collected lab result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, SEX, LBNAM, LBSTRESN, LBORRESU,
LBSTNRLO, LBSTNRHI, LBNRIND

DOMAIN is "LB". STUDYID is the ODM study OID and USUBJID is the ODM subject
key, each as given. Each record is built from one IT.LB.LBDTC item of a lab
panel, with the panel's other items read alongside it, and LBSEQ is the
panel's repeat key. LBTESTCD is the test code from the IT.LB.LBTESTCD item.
SEX is the recorded sex from the IT.DM.SEX item. LBNAM is the collecting lab
from the IT.LB.LBNAM item. LBSTRESN is the numeric result in standard units
from the IT.LB.LBSTRESN item, with no value when the result was not
collected. The reference entry for a result is the LBRANGE entry for the
result's lab, test code, and sex whose age band holds the subject's age in
whole years at collection, counted from the subject's birth date to the
collection date, and which was in effect on the collection date. A subject
whose age falls on an age-band boundary takes the band that starts at that
age. An entry is in effect on a date from its first effective date through
its last effective date, and an entry with no last effective date remains in
effect. When a lab revises a range, the entry in effect on the collection
date wins, so a result collected on the first day of the new range is judged
against the new limits. LBORRESU is the matched entry's unit, LBSTNRLO is
its lower reference limit in standard units, and LBSTNRHI is its upper
reference limit in standard units; all three have no value when no entry
matches. LBNRIND is "LOW" when the result is below the lower limit, "HIGH"
when it is above the upper limit, and "NORMAL" otherwise, and it has no
value when the result or either reference limit is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
