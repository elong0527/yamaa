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

Remove the backward `parent` reference so that the chain ends at
a root layer that names no parent. Keep genuinely shared
definitions in that common ancestor.
