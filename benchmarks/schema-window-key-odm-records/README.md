# Numbered Measurements Each Read Their Own Item Group

**Goal:** show that numbering each subject's collected measurements
(`VSSEQ`) makes every measurement its own record, so the collection date
(`VSDTC`) it takes is the date collected beside that measurement, in the
same group of collected items.

**Input:** one `spec.yaml` over collected study data in operational data
model form (`input/odm.csv`: one record per collected item, with the item's
identifier, value and position in the study, event, form and item group).
Each vital signs item group holds a collection date and one or two blood
pressure measurements.

**Variables:**

- `VSSEQ` numbers each subject's blood pressure measurements from 1 by item
  group; measurements of one group keep their collected order.
- `VSTESTCD` is the test, `SYSBP` or `DIABP`, taken from the item's
  identifier.
- `VSORRES` is the measurement as collected.
- `VSDTC` is the date collected in the measurement's own item group, and
  blank when that group has no date.

**Note:** only the blood pressure items become records. A record's date is
looked up among all collected items, so the date item of its group is found
although it is not a record itself, and the second group of a subject has
its own date.
