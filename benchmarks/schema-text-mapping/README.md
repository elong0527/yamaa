# Text Mapping

**Goal:** pin the text lookup contract: inline dictionaries, an external
dictionary file, case-insensitive reads, and a regular-expression
search with capture groups.

**Input:** `DM` carries one record per subject with coded sex, race, and
treatment group values and a subject identifier that normally starts with
a site prefix, such as `UCSD-0123`. The race dictionary is a YAML file
beside the input.

**Columns:**

- `SEXC` reads an inline dictionary with exact matching: `M` gives
  `Male` and `F` gives `Female`, while a code with no entry and a
  missing sex both give `Unknown`.
- `RACEC` uses the same lookup contract with the dictionary loaded
  from the YAML file instead of written in the spec; a race with no
  entry or a missing race gives `Unknown`.
- `TRTGRPC` reads case-insensitively: source and dictionary keys are
  folded to upper case before comparison, so lowercase and mixed-case
  treatment codes match their uppercase keys. A missing treatment group
  gives `Unknown`.
- `SITENUM` matches each whole identifier against a regular expression
  (capital letters, a hyphen, digits) and keeps the second capture
  group, the digits after the prefix, as text, so leading zeros stay.
  An identifier of any other shape gives `NONE`, and a missing
  identifier gives `MISSING`.
