Following CDISC ADaM standards, use the provided ADSL and RS datasets to create
an ADRS dataset with one record per investigator-scored overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RSSEQ, PARAMCD, PARAM, RSDTC, ADT, ADY, AVALC, AVAL, ANL01FL

- A record appears only for an assessment the investigator scored as
  overall response, with RSTESTCD "OVRLRESP" and RSEVAL "INVESTIGATOR".
- A target-lesion assessment (RSTESTCD "TRGRESP") and an assessment
  scored by an independent assessor (RSEVAL "INDEPENDENT ASSESSOR") leave
  no record.
- RSSEQ is the sequence number of the assessment within the subject.
- PARAMCD is "OVR" and PARAM is "Overall Response by Investigator" on
  every record.
- RSDTC holds the assessment date exactly as collected, even when only a
  year or a year and a month were collected.
- ADT is the completed analysis date: a fully collected date is used as
  it stands, a year and month without a day complete to the first of the
  month, and a year alone completes to 1 January.
- A completed date counts exactly as a collected one in every
  comparison, so the study day and the per-date flag never depend on how
  much of the date was collected.
- ADY is the study day of the assessment, counting the treatment start
  as day one, so an assessment dated exactly on the treatment start
  lands on study day one.
- AVALC is the collected response, one of "CR", "PR", "SD",
  "NON-CR/NON-PD", "PD", or "NE".
- The codes stand for complete response ("CR"), partial response
  ("PR"), stable disease ("SD"), neither complete response nor
  progressive disease ("NON-CR/NON-PD"), progressive disease ("PD"), and
  not evaluable ("NE").
- AVAL ranks the response from best to worst: 1 for "CR", 2 for "PR",
  3 for "SD", 4 for "NON-CR/NON-PD", 5 for "PD", and 6 for "NE".
- ANL01FL is "Y" for one record at each assessment date (ADT): the
  record carrying the worst response that day, the largest AVAL, with
  the lowest RSSEQ breaking ties when a day carries the same worst
  response twice.
- ANL01FL has no value on every other record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
