Following CDISC SDTM standards, use the provided ODM dataset to create an EG
dataset with one record per test of each replicate ECG taken at a planned
time point.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EGSEQ, EGTESTCD, EGTEST, EGORRES, EGORRESU, EGSTRESC,
EGSTRESN, EGSTRESU, EGREPNUM, VISIT, EGDTC, EGTPT, EGTPTNUM

DOMAIN is "EG". STUDYID is the ODM study OID and USUBJID is the ODM subject
key, each as given. VISIT is "DAY 1", the visit the ECGs were taken at. Each
planned time point has its own ECG form: EGTPT is "PRE-DOSE" with EGTPTNUM 1
on the pre-dose form, and "2 H POST-DOSE" with EGTPTNUM 2 on the two-hour
post-dose form. Each replicate ECG taken at a time point is one repeat of the
ECG item group, and EGREPNUM is that item-group repeat key: it numbers the
replicate ECGs of one time point, starting again at each time point.
Each ECG gives one record for its QT interval, with EGTESTCD "QTAG" and
EGTEST "QT Interval, Aggregate", and one for its overall interpretation,
with EGTESTCD "INTP" and EGTEST "Interpretation". A result left blank on the
form gives no record, while the other result of the same ECG keeps its
record. EGORRES is the result as collected: the interval in milliseconds, or
the interpretation. EGSTRESC is the collected result as text for both tests,
since the interval was collected in standard units and the interpretation in
standard terms. EGSTRESN is the interval as a number and has no value for
the interpretation. EGORRESU and EGSTRESU are "msec" for the interval and
have no value for the interpretation. EGDTC is the ECG's date and time, from
the same repeat, on both of its records. EGSEQ numbers each subject's
records from 1 by time point number, then replicate number, then test code
(INTP before QTAG).

Read the source datasets from /app/input and save the completed dataset as
/app/output/eg.csv.
