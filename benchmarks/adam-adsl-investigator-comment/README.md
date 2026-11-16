# Keep Investigator Comments Exactly as Collected

**Goal:** keep each subject's investigator comments in `CMNT` and
whether any was collected in `CMNTFL`.

**Input:** demographics (DM) rows listing the subjects, plus SDTM
comments (CO) records, one per comment, carrying the comment text
(`COVAL`), its continuation (`COVAL1`) and the comment's sequence
number (`COSEQ`). A comment longer than the 200 characters a transport
file allows continues in `COVAL1`, split between words.

**Variables:**

- `CMNT` is the subject's comments exactly as the investigator
  recorded them: a comma inside a comment, quotation marks around a
  subject's own words, and a line break in the middle of a comment all
  reach the result unchanged. A comment split across `COVAL` and
  `COVAL1` is joined back with one space; an empty `COVAL1`, quoted or
  not, adds nothing. A subject's comments are joined in `COSEQ` order,
  separated by a semicolon and a space. It is absent when the subject
  has no comment.
- `CMNTFL` is `Y` when the subject has a comment and `N` when none was
  collected.
