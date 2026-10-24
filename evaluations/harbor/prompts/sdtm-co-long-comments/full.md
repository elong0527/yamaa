Following CDISC SDTM standards, use the provided CO_RAW dataset to create a CO
dataset with one record per collected comment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, COSEQ, COVAL, COVAL1, COVAL2

COSEQ is the comment number as collected. A record whose comment was left
blank gives no record. The comment is cut into pieces of 200 characters at
exact character positions, not between words, so a word can run from one
piece into the next. COVAL is the first 200 characters of the comment, or the
whole comment when it has 200 characters or fewer. COVAL1 is characters 201
to 400 of the comment, and COVAL2 is characters 401 to 600; either piece is
shorter when the comment ends inside it, and has no value when the comment
ends before it begins.

Read the source datasets from /app/input and save the completed dataset as
/app/output/co.csv.
