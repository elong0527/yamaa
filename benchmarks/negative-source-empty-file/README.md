# Reject an Empty Export File

**Goal:** carry the site name (`SITENM`) onto a subject listing.

**Input:** a demographics export that holds no bytes at all, not even the
header record that names its fields.

**Variables:**

- `SITENM` would be the name of the site the subject enrolled at, taken from
  the collected value.

A listing that holds its header and no records says that no subject has been
collected. A file with no bytes says nothing: the export may have failed, or
been cut short before it wrote its first line, so an empty listing read from
it could hide subjects the study has collected. The run is rejected while
the file is read, and no artifact is accepted.

## How to fix

Export the listing again and check that the program finished. A study with
no subjects yet still exports the header, which gives an empty listing:

    STUDYID,USUBJID,SITENM
