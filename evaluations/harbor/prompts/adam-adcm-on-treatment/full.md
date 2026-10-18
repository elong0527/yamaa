Following CDISC ADaM standards, use the provided ADSL and CM datasets to create
an ADCM dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, ASTDT, AENDT, TRTSDT, TRTEDT, ONTRTFL

ASTDT is the medication's analysis start date, the date collected as
CMSTDTC, and AENDT is the medication's analysis end date, the date
collected as CMENDTC. TRTSDT is the subject's first treatment date and
TRTEDT the subject's last treatment date, both from the subject's
subject-level record; a subject whose subject-level record has no
treatment dates has no value for TRTSDT or TRTEDT. ONTRTFL is "Y" when
the medication's dates overlap the subject's treatment period, from
TRTSDT through TRTEDT, and has no value otherwise. A medication that
ends before TRTSDT, or that starts after TRTEDT, has no value for
ONTRTFL; a medication that starts on TRTEDT or ends on TRTSDT does
overlap. A medication with no ASTDT or no AENDT is treated as
overlapping unless its known dates rule overlap out, so a medication
with no ASTDT that ends before TRTSDT has no value for ONTRTFL. A
missing TRTEDT leaves the treatment period open-ended. A subject with
no TRTSDT is never flagged: ONTRTFL has no value on every record for
that subject.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
