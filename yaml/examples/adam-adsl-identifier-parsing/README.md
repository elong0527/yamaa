# ADaM ADSL: parse the site from USUBJID with a collected fallback

This example uses sample DM data and a `yamaa` specification to derive one row
per subject:

- `SUBJID` is the subject number as collected;
- `SITEIDP` is the site read out of the middle of `USUBJID`, which is formed as
  study, site, and a four-digit subject number. A `USUBJID` not in that form
  leaves it empty;
- `SITEID` is the site to use: the parsed value when there is one, otherwise
  the collected site, and `UNKNOWN` when there is neither. Publishing both it
  and `SITEIDP` shows which subjects fell back;
- `SUBJREF` is a display reference combining `SITEID` and `SUBJID`, separated
  by a colon. A subject with no subject number gets `UNKNOWN` in its place
  rather than a partial reference.
