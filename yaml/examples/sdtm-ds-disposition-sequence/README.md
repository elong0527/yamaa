# SDTM DS: number each subject's disposition records in date order

This example uses raw disposition records and a `yamaa` specification to
derive one record per collected disposition:

- `DSSEQ` numbers a subject's records by the date they occurred, earliest
  first. A record without a usable date is still numbered after the subject's
  dated records;
- `DSDECOD` is the standardized outcome the form recorded: a completion, a
  discontinuation reason such as an adverse event, a randomization, or a
  screen failure;
- `DSCAT` is the category the outcome belongs to: a randomization is a
  protocol milestone, and every other outcome is a disposition event;
- `DSDTC` is the date the outcome occurred. A record whose form did not name a
  complete calendar day has none.
