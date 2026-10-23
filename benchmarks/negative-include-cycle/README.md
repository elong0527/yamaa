# Reject a Template That Adds the Study File

**Goal:** prepare the analysis start date of each adverse event on top of the
study's adverse event template.

**Input:** collected adverse events, one record per event, and the template
in `layers/ae.yaml`, on which the entry builds.

**Variables:**

- `ASTDT` would be the analysis start date of the event, as collected.

**Note:** the template adds the entry file itself on top of its own
definitions, while the entry builds on the template. Each then has to be
complete before the other, so no stable set of definitions can be reached,
and the run is rejected before any data is read.

## How to fix

Remove the entry file from the template's `include` list. A template lists
only reusable files that name no file to build on; the study's entry builds
on the template by naming it as its `parent`, never the other way round.
