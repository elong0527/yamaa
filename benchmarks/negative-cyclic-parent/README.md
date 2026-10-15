# Reject Cyclic Parent

**Goal:** prepare subject records from definitions shared
through a reusable file.

**Input:** demographics (DM) records identified by subject.

**Variables:**

No variables are requested, so no output is produced. The entry
file reuses `layers/parent.yaml`, which points back to the entry
file, so no stable set of definitions can be reached, and the run
is rejected before any data is read.

## How to fix

Remove the backward `parents` reference so that every path
through the chain terminates. Keep genuinely shared definitions
in one common ancestor.
