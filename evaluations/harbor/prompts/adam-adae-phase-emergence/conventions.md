Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDT, TREM01FL, TREM02FL, TREM03FL,
TRTEMFL

The study has three phases, each with its first and last dates in ADSL:
neoadjuvant treatment (TR01SDT, TR01EDT), surgery (TR02SDT, TR02EDT) and
adjuvant treatment (TR03SDT, TR03EDT). TREM01FL, TREM02FL and TREM03FL are
the flags of the neoadjuvant, surgery and adjuvant phases, and each is "Y" or
has no value; TRTEMFL is "Y" or has no value. Days after a phase's last date
are counted with the end day included, for both the 30-day and the 90-day
limit: an event 30 days after the last date is within 30 days, one 31 days
after it is not.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
