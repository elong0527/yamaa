Following CDISC ADaM standards, use the provided ADPP_RAW, ADSL and
COMPARE datasets to create an ADPP dataset with one record per PK
parameter per subject per period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL, ANL01FL,
ANL02FL

COMPARE lists the planned treatment comparisons: COMPNO numbers each
one, TESTTRT is its test treatment and REFTRT its reference treatment,
written as in TRTA. ANL01FL marks the records comparison 1 uses and
ANL02FL the records comparison 2 uses: the flag numbered nn belongs to
the comparison whose COMPNO is nn.

A comparison uses only the records of its own two treatments, and only
complete pairs. A record is Y in a comparison's flag when its treatment
is one of the comparison's two treatments, its own AVAL is evaluable,
and the same subject has an evaluable AVAL for the same parameter under
the comparison's other treatment. Evaluable means present and greater
than 0. Pairs are judged per parameter, so a subject can be in a
comparison for one parameter and out of it for another. A treatment
that is the test of one comparison and the reference of the other can
be Y in both flags. A flag has no value on every other record, and all
records stay in the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
