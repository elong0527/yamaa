# Split Long Comments into 200-Character Pieces

**Goal:** derive the comment `COVAL` and its continuations `COVAL1` and
`COVAL2`.

**Input:** collected free-text comments, one record per comment, with the
subject and the comment number (`COSEQ`).

**Variables:**

- `COVAL` is the first 200 characters of the comment, or the whole comment
  when it is shorter.
- `COVAL1` is characters 201 to 400 of the comment, or the rest of it when
  the comment ends sooner; it is absent when the comment has 200 characters
  or fewer.
- `COVAL2` is characters 401 to 600 of the comment, or the rest of it when
  the comment ends sooner; it is absent when the comment has 400 characters
  or fewer.

**Note:** the comment is cut at fixed character positions, not between
words, so a word can run from one piece into the next. A comment left blank
gives no comment record, and a comment longer than 600 characters stops the
run, since three pieces hold no more.
