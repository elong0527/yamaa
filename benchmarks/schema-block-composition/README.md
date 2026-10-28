# Block Composition

**Goal:** show a resubmission level that changes only the list of
published variables and the dataset file its data definition names,
inheriting every other part of the dataset's layout and of its received
description from the original package.

**Dataset:** `dm.csv`, SDTM demographics. One record per subject.

**Input:** two spec files, read root-first:

- `spec_package.yaml` carries the original package: the age, its units
  and the sex collected for each subject, published in subject order to
  `dm.csv`, and the dataset's description as the package received it,
  a special-purpose demographics dataset of one record per subject held
  in the SAS transport file `dm.xpt`;
- `spec_resubmission.yaml` is the entry: it names the original package
  as its parent, publishes its variables without the age units, and
  names its dataset file `dm.json`. It writes nothing else.

**Variables:**

- `DOMAIN` is always `DM`.
- `AGE` is the subject's collected age in years.
- `SEX` is the subject's collected sex.

**Note:** a level that changes one part of the published layout or of
the received description writes that part alone, and every part it
leaves out keeps its inherited value. The resubmission therefore keeps
the original file name and subject order, and its data definition
describes the shorter variable list with the received description and
the new dataset file.
