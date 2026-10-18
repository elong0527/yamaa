Following CDISC SDTM standards, use the provided ODM dataset to create a PR
dataset with one record per prior-procedure form.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC, PRLAT,
PRDOSE, PRDOSU, PRSTDTC, PRENDTC

DOMAIN is "PR". STUDYID is the ODM study OID as given, and USUBJID is
the study OID and the subject key joined with a hyphen. One record is
written for each prior-procedure form collected for a subject: the
prior cancer surgery form and the prior radiotherapy form. PRTRT is
the procedure name as reported on the form. PRCAT is "PRIOR CANCER
SURGERY" on the surgery record and "PRIOR RADIOTHERAPY" on the
radiotherapy record. The radiotherapy form asks the pre-specified
question "Any prior radiotherapy?"; PRPRESP is "Y" on the radiotherapy
record and has no value on the surgery record. PROCCUR is the answer
to that question, "Y" or "N", on the radiotherapy record and has no
value on the surgery record. The radiotherapy record is written even
when the answer is "N": a "No" answer is still a record, and it
carries no dose and no dates. PRLOC is the location of the procedure
and PRLAT its laterality, as collected on the surgery form; both have
no value on the radiotherapy record. PRDOSE is the dose per
administration of the radiotherapy and PRDOSU its unit, as collected
when the radiotherapy occurred; both have no value when the procedure
did not occur and on the surgery record. PRSTDTC is the start date of
the procedure and PRENDTC its end date, each keeping the precision
collected, so a date collected as a year and month stays a year and
month. PRSEQ numbers each subject's records from 1 by start date, then
by procedure name.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pr.csv.
