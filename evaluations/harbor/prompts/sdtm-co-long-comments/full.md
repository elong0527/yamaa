Following CDISC SDTM standards, use the provided CO_RAW dataset to create a CO
dataset with one record per subject per collected comment number.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, COSEQ, COVAL, COVAL1, COVAL2

DOMAIN is "CO". COSEQ is the comment number as collected, not renumbered. A
comment left blank gives no CO record.

The comment text is held in 200-character pieces cut at fixed character
positions, not between words, so a word can run from one piece into the next:

- COVAL is the first 200 characters of the comment, or the whole comment when
  it is shorter.
- COVAL1 is characters 201 to 400 of the comment, or the rest of it when the
  comment ends sooner; it has no value when the comment has 200 characters or
  fewer.
- COVAL2 is characters 401 to 600 of the comment, or the rest of it when the
  comment ends sooner; it has no value when the comment has 400 characters or
  fewer.

Read the source datasets from /app/input and save the completed dataset as
/app/output/co.csv.
