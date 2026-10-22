# Refuse a Data Definition That Points Outside Its Package

**Goal:** submit the subject-level analysis dataset with its data
definition, written into a package folder.

**Input:** the demographics domain with the age (`AGE`), one record per
subject.

**Variables:**

- `AGE` is the age in years, copied from the demographics domain.

**Note:** the data definition is written into the folder `package`, while
the dataset it describes stays beside the project, one folder up. A data
definition points only at files inside its own folder, the package a
reviewer receives, so the submission is refused before any document is
written; the dataset itself still builds.

## How to fix

Keep the data definition beside the datasets it describes:

```yaml
adam:
  define: define.xml
```

or save the dataset into the package folder, with `output.path` set to
`package/adsl.csv`.
