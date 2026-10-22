# Reject an Item Defined Twice With Two Names

**Goal:** build one Demographics (DM) record per subject with the sex
(`SEX`) collected on the demographics form.

**Input:** the electronic data capture (EDC) system's export as an
Operational Data Model (ODM) 1.3 XML file with its study design, in which
item `IT.DM.SEX` of design version `MDV.1` is defined twice, once named
`Sex` and once named `Gender`.

**Variables:**

- `SEX` would be the subject's collected sex code.

Each collected item takes its name from its definition in the study design.
One item with two names in one version of the design has no single name,
and keeping either would label the subjects' answers with a name the design
also contradicts. The input is rejected while it is read, and no artifact is
accepted.

## How to fix

Correct the study design in the EDC system so that the item is defined
once, with one name, and export again:

```xml
<ItemDef OID="IT.DM.SEX" Name="Sex" DataType="text" Length="1"/>
```

An item renamed in a later version of the design belongs to that version,
which the export writes as its own `MetaDataVersion`.
