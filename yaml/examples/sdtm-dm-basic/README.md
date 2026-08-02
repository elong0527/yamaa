# SDTM DM basic mapping

This is the smallest complete ODM-to-SDTM example. Its input has the same
long-form columns produced by the ODM XML parser.

The `odm.csv` fields follow the ODM 2.0 clinical-data hierarchy. The `subject`
row template selects the required sex item as one anchor record per subject.
It reads `SEX` from the anchor's `ODM.Value` and resolves `ODM.IT.DM.AGE` and
`ODM.IT.DM.ARM` using the same study, subject, event, item-group, and repeat-key
context. The six input item rows therefore produce two DM rows.

It demonstrates:

- a direct ODM context source for `STUDYID` and a literal for `DOMAIN`;
- direct source mapping from ODM context and full `ItemOID` references;
- controlled terminology mapping for `SEX`;
- integer conversion for `AGE`; and
- an unqualified current-dataset reference, `ACTARM` from `ARM`.

The combined values of `STUDYID` and `USUBJID` must be unique and non-missing.
The output column order follows the order in `spec.yaml`.
