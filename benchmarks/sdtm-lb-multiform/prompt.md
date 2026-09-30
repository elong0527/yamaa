Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per reported result from the serum,
skin-biopsy, saliva, and tape-strip forms.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, VISIT, VISITNUM, LBTESTCD, LBTEST, LBCAT,
LBSPEC, LBLOC, LBORRES, LBORRESU, LBSTRESC, LBSTRESN, LBSTRESU, LBSTAT,
LBDTC

LBSEQ numbers each subject's records by collection date, then test code
and specimen; lesional results come before non-lesional ones from the same
form. VISITNUM is 1 for screening, 2 for baseline, and 3 for day 21; each
unscheduled visit is numbered after the baseline visit per occurrence
(2.01, 2.02). VISIT is the matching visit name.

LBTESTCD is VITD25OH, IL13, or CAMPPRO. LBTEST is 25-Hydroxyvitamin D,
Interleukin 13 mRNA, or Cathelicidin Protein. LBCAT is CHEMISTRY, GENE
EXPRESSION, or ANTIMICROBIAL PEPTIDE. LBSPEC is SERUM, SKIN BIOPSY,
SALIVA, or TAPE STRIP. LBLOC is LESIONAL or NON-LESIONAL for biopsy and
tape-strip results, and has no value for serum and saliva results.

LBORRES is the collected result, exactly as reported, and has no value
when the test was not done. LBORRESU is CYCLE for biopsy results and ng/mL
otherwise, and has no value when the test was not done. LBSTRESC repeats
the reported result in standard form; LBSTRESN is its numeric form; and
LBSTRESU repeats the unit. All three have no value when the test was not
done. LBSTAT is NOT DONE when the test was not done, and has no value
otherwise. LBDTC is the collection date from the date item on the same
form.

An item with no reported value produces no record, while a collected zero
is kept as a real result. An entry of NOT DONE still produces a record,
with no value in the result and unit fields. A form collected twice at one
visit keeps each occurrence's own date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
