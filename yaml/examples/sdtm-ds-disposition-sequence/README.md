# SDTM DS: number each subject's disposition records in date order

This example uses raw disposition records and a `yamaa` specification to
derive one record per collected disposition:

- `DSDECOD` is the standardized outcome the form recorded: a completion, a
  discontinuation reason such as an adverse event, a randomization, or a
  screen failure;
- `DSCAT` is the category the outcome belongs to: a randomization is a
  protocol milestone, a screen failure is a disposition event, and every
  remaining outcome reports how the study ended for the subject;
- `DSDTC` is the date the outcome occurred. A record whose form left the date
  blank has none;
- `DSSEQ` numbers a subject's records by the date they occurred, earliest
  first. A subject whose only record has no date is still numbered, after any
  dated ones.
