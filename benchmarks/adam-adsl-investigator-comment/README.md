# Keep an Investigator Comment Exactly as Collected

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

**Note:** an empty comment field, quoted or not, counts as no
comment collected: the flag is `N` and the comment is absent. A
field holding only spaces is still a comment: the flag is `Y` and
the spaces are kept.
