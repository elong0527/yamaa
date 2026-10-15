# Parse the Site from the Subject Identifier

**Goal:** derive the parsed site, the site to use, and the display
reference (`SITEIDP`, `SITEID`, and `SUBJREF`) for each subject.

**Input:** demographics with the study identifier, the unique subject
identifier, the collected subject number, and the collected site.

**Variables:**

- `SITEIDP` is the site code read from the middle of the unique
  subject identifier when it has the form `YAMAA-<site>-<four digits>`;
  empty when the identifier has any other shape.
- `SITEID` is the site to use: the parsed site when present, otherwise
  the collected site, otherwise `UNKNOWN`.
- `SUBJREF` combines the site to use and the collected subject number
  with a colon; `UNKNOWN` when the subject number is missing.

**Note:** keeping both the parsed and the final site shows which
subjects fell back to the collected site and which had no usable site
at all.
