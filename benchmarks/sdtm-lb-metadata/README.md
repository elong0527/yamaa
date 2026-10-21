# Value-Level Metadata

**Goal:** build one record per collected glucose and creatinine
result, with per-test submission metadata: each `LBTESTCD` value
gets its own codelist and origin for `LBORRESU`, overriding the
shared column-level declaration.

**Input:** long-form Operational Data Model (ODM) rows, one row
per collected item, with the collected entry in the value field.
Glucose rows carry one item, creatinine rows carry another, and
the collection date rides along in the same visit group.

**Variables:**

- `LBTESTCD` is `GLUC` for glucose rows and `CREAT` for
  creatinine rows.
- `LBTEST` is `Glucose` when the test code is `GLUC` and
  `Creatinine` when it is `CREAT`.
- `LBORRES` is the collected entry, kept exactly as reported.
- `LBORRESU` is `mg/dL` for every record. Its submission
  metadata is stated once for the whole column and overridden for
  each test code: glucose records name the glucose codelist with a
  vendor origin, creatinine records the creatinine codelist with an
  investigator origin.
- `LBSTRESN` is the numeric form of the collected entry; missing
  when the entry is not a number.
- `LBSTRESU` is `mg/dL` for every record.
- `LBDTC` is the collection date from the same visit group.

**Note:** a test with no collected entry produces no record, so the
records are only results actually reported. The per-value
submission metadata is validated when the specification loads;
the engine output is unaffected by it.

The case runs `submission-spec.yaml`, an independently completed submission
layer over the original executable `spec.yaml`, with `submission-project.yaml`,
the project file that declares the unit codelists both layers bind, so every
unit is checked against its list. The completion adds complete dataset/column
metadata and retains the glucose and creatinine codelist declarations. It
restates both test codes' entries, since a later layer replaces the whole
list, and explicitly uses Sponsor for the creatinine Assigned origin: SDTM
admits Vendor or Sponsor for Assigned, whereas the original family-independent
loader fixture retains its Investigator declaration. Installed Define tests
also exercise that original declaration's family-admission failure.

The installed Define tests' fixed-time witness reads the unchanged
`expected/lb.csv` only when Dataset-JSON is explicitly requested; it does not
execute the specification or open its ODM input. Its independently authored
XML structure is validated against the bundled offline Define-XML XSD. The
original declarations, input, CSV expectation and reference loader controls
retain their existing meaning.
