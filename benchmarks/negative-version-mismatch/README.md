# Reject a Parent Written for a Newer Language Version

**Goal:** prepare subject records from definitions shared
through a reusable file.

**Input:** demographics (DM) records identified by subject.

**Variables:**

No variables are requested: the entry file names no output
variables.

**Note:** the entry file reuses `layers/parent.yaml`, which states
that it needs language version `2.0`. This engine reads only version
`1.0`, so it cannot read the shared definitions: the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Find out which language version the parent really needs. A file declares the
oldest version whose features it uses, and this parent uses nothing newer
than `1.0`, so `"1.0"` is the value to correct it to:

```yaml
yamaa: "1.0"
type: spec
```

The entry file and its parent may declare different versions. A parent that
does need a newer version needs an engine that reads it; raising the entry
file to `"2.0"` as well is rejected the same way.
