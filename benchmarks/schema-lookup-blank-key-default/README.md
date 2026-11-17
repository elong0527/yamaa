# A Study-Wide Qualifier With Subject Exceptions

**Goal:** one row per subject carrying the intent-to-treat population flag
(`ITTFL`) from the supplemental qualifiers of the demographics.

**Input:** one `spec.yaml` over the subjects' demographics and their
supplemental qualifiers. A qualifier record may leave the subject blank, to
set the flag for the whole study, or name the subject it applies to.

**Variables:**

- `ITTFL` is the subject's own non-missing intent-to-treat flag when a
  qualifier record names the subject, and otherwise the flag of the record
  that leaves the subject blank. A qualifier of another name, such as the
  safety flag, does not set it.

**Note:** a record whose subject is blank names no subject, so it is not found
by looking up a subject. It is found by looking up the study alone and keeping
only the records whose subject is blank, which every subject of the study then
reads. A subject's own non-missing flag takes precedence over it; a missing
subject flag falls through to the study flag.
