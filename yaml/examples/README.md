# Derivation schema examples

These fixtures exercise `yaml/schema.yaml` with small inputs and exact expected
outputs. They are intended for human review, automated tests, and AI-assisted
implementation.

Execution behavior is defined by the normative rules in
[`../rules/README.md`](../rules/README.md). Example READMEs describe only the
fixture-specific application of those rules.

`odm.csv` is a tabular projection of ODM clinical data, not an ODM exchange
document itself. Its fields map to the official
[CDISC ODM 2.0 clinical-data schema](https://github.com/cdisc-org/DataExchange-ODM/blob/main/schema/ODM-clinicaldata.xsd).

Dataset declarations, variable references, and ODM contextual lookups are
governed by [R002](../rules/R002-source-binding.md).

Examples are ordered by increasing complexity:

1. `sdtm-dm-basic` — direct mapping, literals, terminology mapping, and a
   current-dataset reference.
2. `sdtm-lb-findings` — row construction, wide-to-long Findings conversion,
   missing-result filtering, and sequence generation.
3. `sdtm-relrec-related-records` — row construction from multiple source
   datasets for a one-to-many relationship between records.
4. `adam-adlb-bds` — source-dataset enrichment, baseline selection, change from
   baseline, percentage change, and analysis sequence.

Each example contains a specification, source CSV files, an expected CSV, and
a README defining behavior that an implementation must reproduce.

## Schema probes

A probe is a fixture written to test whether the schema can express a real
mapping pattern. A probe that does not pass is a design finding, not a defect in
the fixture, and its README records what the schema could not express.

5. `adam-adsl-mapping` — value mapping with an inline dictionary, deriving
   ADaM numeric companions, including a value the dictionary does not define.
6. `sdtm-ae-dictionary-coding` — value mapping where the dictionary is an
   external file, including an uncoded term and the dictionary version.

Both pass. Together they cover value standardization from both sources of
mapping rule, inline and external, and the undefined-value case in each.

## Coverage gaps

Of the registered vocabulary, 8 of 16 non-leaf expressions and 4 of 5 local
handler paths are not exercised by any fixture:

- expressions: `add`, `case`, `coalesce`, `cut`, `date_diff`, `max`, `min`,
  `str_extract`
- handlers: `conversion_failure`, `source.missing`, `multiple_matches`,
  `override`

Those paths are unverified. The absence of `min` and `max` also leaves
`source.filter` and R003 right-side reduction with no fixture, while the absence
of `case` leaves R001 predicate dependency extraction uncovered. Each should
either gain a fixture or be removed.

Four of seven verification keywords are exercised by `adam-adsl-mapping`:
`not_missing`, `allowed_values`, `unique`, and `row_count`. The unexercised
verification keywords are `range`, `matches`, and `predicate`.
