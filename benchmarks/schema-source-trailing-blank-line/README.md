# Read a One-Column List That Ends with a Blank Line

**Goal:** list each identifier (`ID`) of a one-column list typed by hand.

**Input:** one `spec.yaml` reading `input/ids.csv`, a list of identifiers,
one to a line, saved from a text editor that left a blank line after the
last identifier.

**Note:** blank lines at the end of the file hold no record, whatever the
number of fields, so the listing has one row per identifier and none with a
missing identifier. A blank line before the last identifier is a record
whose identifier is missing.
