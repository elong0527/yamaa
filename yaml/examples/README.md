# Derivation schema examples

These fixtures exercise `yaml/schema.yaml` with small inputs and exact expected
outputs. They are intended for human review, automated tests, and AI-assisted
implementation.

`odm.csv` is a tabular projection of ODM clinical data, not an ODM exchange
document itself. Its fields map to the official
[CDISC ODM 2.0 clinical-data schema](https://github.com/cdisc-org/DataExchange-ODM/blob/main/schema/ODM-clinicaldata.xsd).

Variable references follow two rules:

- `XX.YYYY` reads `YYYY` from source dataset `XX`.
- `YYYY` reads a variable from the current dataset being constructed.

`datasets` maps dataset identifiers explicitly to paths, such as
`ADSL: input/adsl.csv`. The identifiers are used by `base` and qualified
variable references. Paths are relative to the example's `spec.yaml`.

For an `ODM` base, context fields are explicitly qualified, such as
`ODM.StudyOID`, `ODM.SubjectKey`, and `ODM.ItemOID`. A reference such as
`ODM.IT.LB.LBDTC` reads the `Value` whose `ItemOID` is `IT.LB.LBDTC` within the
current ODM context.

Examples are ordered by increasing complexity:

1. `sdtm-dm-basic` — direct mapping, literals, terminology mapping, and a
   current-dataset reference.
2. `sdtm-lb-findings` — row construction, wide-to-long Findings conversion,
   missing-result filtering, and sequence generation.
3. `adam-adlb-bds` — source-dataset enrichment, baseline selection, change from
   baseline, percentage change, and analysis sequence.

Each example contains a specification, source CSV files, an expected CSV, and
a README defining behavior that an implementation must reproduce.
