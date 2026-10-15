# Metadata Contract

**Goal:** build one DM record per subject carrying `SITEID`, `AGE`,
`AGEU`, `SEX`, and `COUNTRY`.

**Input:** one row per subject carrying `SITEID`, `AGE`, `SEX`, and
`COUNTRY`.

**Variables:**

- `USUBJID` joins the study, site, and subject identifiers with hyphens;
  an identifier longer than 30 characters stops the run rather than being
  shortened.
- `SITEID` is the collected site identifier.
- `AGE` is the collected age in whole years, between 0 and 120;
  missing when not collected.
- `AGEU` is fixed to `YEARS`, since age is collected in whole
  years.
- `SEX` is the collected sex, one of `F`, `M`, or `U`.
- `COUNTRY` is the collected three-letter country code.

**Note:** the dataset and its variables carry the description,
labels, provenance, lengths, terminology, class, structure, and
standard version a review needs. The same declarations produce the
data-definition document beside the data and the exchange file
`dm.json`, which carries the rows and the column metadata together and
is the file the document points at.
