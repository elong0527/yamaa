Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDT, TREM01FL, TREM02FL, TREM03FL,
TRTEMFL

The study has three phases, each with its first and last dates in ADSL:
neoadjuvant treatment (TR01SDT, TR01EDT), surgery (TR02SDT, TR02EDT) and
adjuvant treatment (TR03SDT, TR03EDT). A phase the subject never entered has
no dates, and an untreated subject has no phase dates at all.

- ASTDT is the onset date, AESTDTC.
- Each phase starts on its own first date and ends the day before the next
  phase the subject entered starts. The next phase after neoadjuvant
  treatment is surgery or, for a subject with no surgery, adjuvant
  treatment; the next phase after surgery is adjuvant treatment. So an
  event on the day of surgery is a surgery-phase event, and an event in a
  gap between two phases belongs to the earlier phase however long the gap.
- The last phase the subject entered has no next phase. It ends 30 days
  after its last date (TR01EDT, TR02EDT or TR03EDT) for a non-serious event
  (AESER "N") and 90 days after it for a serious event (AESER "Y"), the end
  day included: a non-serious event 30 days after the last date is in the
  phase, one 31 days after it is not. When EOTSTT is "ONGOING", the last
  phase has no end.
- TREM01FL is "Y" for an event that started in the neoadjuvant phase,
  TREM02FL is "Y" for one that started in the surgery phase, and TREM03FL
  is "Y" for one that started in the adjuvant phase. Each has no value for
  any other event.
- TRTEMFL is "Y" when one of the three phase flags is "Y" and has no value
  otherwise.
- An event before TR01SDT, an event after the last phase ends, and every
  event of an untreated subject have no value in any of the four flags.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
