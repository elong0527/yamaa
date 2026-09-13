# Keep an investigator comment exactly as collected

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-investigator-comment.html)

**Goal:** keep the investigator comment in `CMNT` and whether one
was collected in `CMNTFL`.

**Input:** demographics (DM) rows carrying the `COMMENT` field.

**Variables:**

- `CMNT` is the comment exactly as the investigator recorded it: a
  comma inside it, quotation marks around a subject's own words, and
  a line break in the middle of it all reach the result unchanged;
  it is absent when no comment was collected.
- `CMNTFL` is `Y` when a comment was collected and `N` when none
  was.

**Note:** when the flag shows no comment was collected, the comment
itself is absent.

**Standard:** ADaM | **Domain:** ADSL
