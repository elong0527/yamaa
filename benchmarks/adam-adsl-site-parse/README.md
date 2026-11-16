# Check the Site in the Subject Identifier

**Goal:** carry the site (`SITEID`), read the site from the subject
identifier as a check (`SITEIDP`), and build the display reference
(`SUBJREF`) for each subject.

**Input:** demographics with the study identifier, the unique subject
identifier, the collected subject number, and the collected site, which
every subject has.

**Variables:**

- `SITEID` is the site collected in demographics, as it stands.
- `SITEIDP` is the site code read from the middle of the unique subject
  identifier when it has the form `YAMAA-<site>-<four digits>`; empty
  when the identifier has any other shape. It never replaces `SITEID`.
- `SUBJREF` combines `SITEID` and the collected subject number with a
  colon.

**Note:** keeping the site read from the identifier beside the collected
one shows a subject whose identifier names another site, and which
identifiers do not have the expected shape.
