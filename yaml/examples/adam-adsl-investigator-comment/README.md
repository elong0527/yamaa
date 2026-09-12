# ADaM ADSL: keep an investigator comment exactly as collected

This example uses sample DM text and produces one row per subject:

- `CMNT` is the comment the investigator recorded, kept exactly as it was
  collected: a comma inside it, a pair of quotation marks around a subject's
  own words, and a line break in the middle of it all reach the result
  unchanged;
- `CMNTFL` is `Y` when a comment was collected and `N` when none was.

A comment left deliberately blank is still a comment, because the investigator
was asked and answered with nothing. It stays distinct from a comment that was
never collected at all, and only the second of the two is absent.
