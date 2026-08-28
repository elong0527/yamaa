# Plan for ODM -> SDTM -> ADaM assessment examples

## Goal

Add small, executable examples that reveal which derivation features the YAML
language needs for a realistic end-to-end clinical-data pipeline. The examples
should complement, rather than duplicate, the component fixtures already under
`yaml/examples/`.

The suite should answer three questions for every proposed feature:

1. Can the schema express the derivation without host-language code?
2. Do R and Python produce the same rows, values, ordering, and errors?
3. Is the behavior precise enough to be a portable rule rather than an
   implementation convention?

This file is a repository-level plan. Unless the project adopts a new layout,
the executable fixtures should continue to be added under `yaml/examples/`.

## What the CATH review adds

The current fixtures cover direct mapping, inline and external dictionaries,
basic ODM-context lookup, wide-to-long row construction, RELREC row union,
baseline/change calculations, and a project function. CATH adds a much broader
end-to-end surface:

- The ODM is a real ODM 2.0 snapshot with 82 subjects, four study events, 17
  form-like containers, repeated item groups, and 10,503 item values. Its
  parser also supports the ODM 2.0 representation where `FormData` is omitted
  and nested `ItemGroupData` acts as the form container.
- SDTM derivation uses subject-level, Events, and Findings patterns; pivots ODM
  item values by context; unions many test templates; derives sequence and
  study-day variables; and resolves cross-domain dependencies.
- ADaM derivation builds ADSL from DM/EX/DS, BDS datasets from LB/VS/RS, and an
  OCCDS ADAE. It requires filtered first/last record selection, lookup joins,
  baseline propagation, change and percentage change, treatment-emergent flags,
  and hierarchical first-occurrence flags.
- CATH also exposes useful contract-drift cases. Some SDTM specs refer to ODM
  items or forms not present in the extract (for example `PG` versus `PT`, and
  `EX.COMPLIANCE_PCT` versus the collected exposure fields). The BDS ADaM code
  expects `VISIT`, `VISITNUM`, and domain baseline flags that are absent from
  the committed SDTM outputs. Placebo exposure is stored with dose zero while
  ADSL selects treated records using `EXDOSE > 0`. These should become explicit
  validation fixtures instead of being left as silent pipeline assumptions.

## Fixture contract

Each example should normally cover one derivation boundary and contain:

```text
yaml/examples/<id>/
├── README.md
├── input/
│   ├── source dataset
│   └── any lookup datasets
├── spec.yaml
├── expected/<dataset>.csv
├── expected/diagnostics.yaml  # once a contract is defined
└── environment.R or environment.py  # only when testing `function`
```

Keep ODM-to-SDTM and SDTM-to-ADaM fixtures separate so each expected result and
diagnostic has one derivation boundary. Add a multi-stage fixture only when the
pipeline manifest and stage handoff are themselves the behavior under test;
such a fixture must reuse the same intermediate artifact rather than maintain a
second hand-written copy. Until a machine-readable diagnostics schema is
defined, expected handler counts and verification results belong in the README
rather than in an invented `diagnostics.yaml` shape.

The IDs below describe business flows. Unless pipeline orchestration is the
feature being assessed, implement each arrow in a flow as a separate fixture.

Every README should identify:

- the business rule and CDISC record grain;
- the schema and rule IDs being exercised;
- the minimum input rows that cover normal, missing, and edge behavior;
- deterministic row ordering and exact output keys;
- expected handler counts and verification results;
- whether the fixture is `supported`, a `probe`, or intentionally `blocked`;
- any new semantic rule needed if the fixture is blocked.

## Priority 1: common end-to-end cases

These should be implemented first. They represent patterns present in most
interventional studies and provide a usable ODM-to-ADaM path.

| ID | Example and flow | Minimum cases | Feature questions |
|---|---|---|---|
| C01 | **Subject, treatment, and disposition:** ODM DM/EX/DS -> SDTM DM/EX/DS -> ADSL | active and placebo subjects; completed and discontinued subjects; more than one EX record; missing DS | Can a pipeline declare multiple outputs and their dependencies? Can it select the first and last qualifying exposure record and copy associated values? Are placebo administrations treatment records even when the active-ingredient dose is zero? Can ADSL population and end-of-study flags be derived with `case` and `coalesce`? |
| C02 | **Laboratory Findings to BDS:** wide ODM LB -> long SDTM LB -> ADLB | two tests, two visits, one `NOT DONE`, one missing numeric result, one zero baseline | Can row templates transpose items without emitting empty records? Are original and standardized results preserved separately? Can analysis baseline, `BASE`, `CHG`, `PCHG`, and `ASEQ` be derived deterministically? This extends `sdtm-lb-findings` and `adam-adlb-bds` into one chain. |
| C03 | **Vital signs and unit normalization:** ODM VS/DM -> SDTM VS -> ADVS | height once in DM; weight and temperature at visits; mixed C/F and kg/lb units | Does the language support conditional unit conversion, rounding, and reuse of subject-level measurements? Can one source result create an original record and a standardized analysis value without project functions? |
| C04 | **Adverse events to OCCDS:** repeating ODM AE -> SDTM AE -> ADAE | pre-treatment, on-treatment, and post-treatment events; repeated PT within SOC; serious and non-serious rows | Can date intervals derive `TRTEMFL`? Can first-occurrence flags be generated at subject, SOC, and PT levels? Can ordered selection operate only on rows meeting a condition? Are ties deterministic and auditable? |
| C05 | **Visits, epochs, and study day:** ODM event context + TV/TA lookup -> SDTM SV and one Findings domain -> ADaM analysis visit | screening, baseline, scheduled, early/late-window, and unscheduled records | Can visit metadata be joined by an explicit key? Can `date_diff`, `case`, and `add` express the SDTM no-Day-0 rule? Can an observed date be assigned to an analysis window with a deterministic tie rule? Are `EPOCH`, `VISIT`, `VISITNUM`, `AVISIT`, and `AVISITN` separate concepts? |
| C06 | **Controlled terminology and coding provenance:** ODM DM/AE -> SDTM DM/AE -> ADSL/ADAE | case variation, missing value, unmapped value, one approved correction, and a duplicate dictionary key in a negative companion | Can inline and external mappings share one lifecycle? Can dictionary name/version and variable origin be carried as governed metadata? Is an unmapped value fatal unless handled? Does a final `override` apply after conversion, use first-match semantics, and report its use count? This extends the existing mapping fixtures with end-to-end provenance. |
| C07 | **SUPPQUAL linkage:** repeating ODM MH and EX -> SDTM MH/EX + SUPPMH/SUPPEX | two parent records per subject and two non-standard qualifiers per record | Can the schema reshape qualifier columns into QNAM/QVAL rows? Can it link them using the parent sequence after that sequence is assigned? Are explicit join keys such as repeat key plus subject supported without silently deduplicating the right side? |
| C08 | **Reference-date bootstrap and participation bounds:** ODM DM plus derived EX/DS/AE -> SDTM `_DM_REF`/EX/DS/DM -> ADSL | consent before treatment, multiple qualifying exposures, early discontinuation, AE after last dose | Can an explicitly declared intermediate dataset break the reference-date dependency without creating a cycle? Can filtered `min`/`max` derive RFSTDTC/RFXSTDTC/RFXENDTC/RFENDTC and last participation? Can an extreme record return both the extreme date and values from the same record? Can the result distinguish no match from multiple matches? |
| C09 | **Identifier parsing and fallback:** ODM subject/site identifiers -> SDTM DM -> ADSL | standard compound ID, missing alternate ID, malformed ID, and one value with an optional prefix | Can `str_extract` distinguish missing and no-match paths and select capture groups? Can `coalesce` apply a fallback? Is a portable concatenate/format operation needed for `USUBJID`? Are all handler counts visible? |

## Priority 2: challenge cases

These cases should deliberately pressure the language where CATH or common
submission work exceeds straightforward column mapping.

| ID | Example and flow | Stress condition | Capability to assess |
|---|---|---|---|
| X01 | **CATH multi-form biomarker consolidation:** ODM LB/BX/SAL/TS -> one SDTM LB -> ADLB | serum chemistry, gene expression, saliva, and tape-strip results use different forms, dates, units, specimens, and anatomical locations | Reusable Findings templates or definitions; union of heterogeneous row drivers; conditional row generation; parameter metadata lookup; stable sequence after union; avoidance of large repeated YAML blocks. |
| X02 | **Conditional compartments and structural missingness:** CATH biopsy -> LB/ADLB | non-AD subjects have non-lesional samples only; AD/psoriasis have lesional and non-lesional samples | Distinguish a structurally inapplicable item from a missing collected value; do not create phantom Findings rows; support conditional parameter availability and corresponding validation. |
| X03 | **Irregular visits and closest-record selection:** ODM unscheduled labs -> SDTM LB -> ADLB | two observations are equally close to the target day; one is before and one after; one lies outside the window | Non-equi/date-distance matching, window bounds, preference rules, deterministic tie handling, and selection audit. Current `min`/`max` is not sufficient for closest-to-target selection. |
| X04 | **Partial and malformed dates:** ODM AE/CM/DS -> SDTM -> ADSL/ADAE | year-only, year-month, complete date, invalid text, unknown day, and conflicting start/end precision | ISO 8601 parsing, precision preservation, imputation rules and flags, interval comparisons under uncertainty, conversion handlers, and whether date and datetime need distinct declared types. |
| X05 | **Repeated and corrected ODM data:** ODM transactions -> SDTM AE/LB | inserted, updated, removed, and duplicate item data; audit timestamps disagree with document order | Selection of the effective record, transaction/audit provenance, context uniqueness, and fatal ambiguity. This should establish whether snapshots only are in scope or whether transactional ODM must be supported. |
| X06 | **Multi-period or crossover treatment:** ODM EX -> SDTM EX/EC -> ADSL + BDS | two treatment periods, washout, treatment switch, and measurements at period boundaries | Period-aware keys, treatment phase assignment, multiple TRTxx variables, interval joins, and prevention of a subject-level automatic join from collapsing period records. |
| X07 | **Many-to-many record relationships:** ODM AE/CM -> SDTM AE/CM/RELREC -> ADAE traceability | one AE links to two medications and one medication links to two AEs; group-level relationship included | Explicit relationship keys, row union, group-level deduplication, referential-integrity verification, and preservation of one-to-many versus many-to-many semantics. This extends the existing individual-record RELREC fixture. |
| X08 | **Responder and composite endpoint:** SDTM RS/LB/DS -> ADRS | response needs percent change, a safety condition from another domain, visit selection, and a discontinuation rule | Multi-dataset conditional derivation, selected-record analysis, categorical and numeric analysis values, missing-component policy, and traceable intermediate calculations. |
| X09 | **Hierarchical occurrence and worst-case selection:** SDTM AE -> ADAE | first event per subject/SOC/PT plus maximum severity with same-day ties | Generic ordered flags, rank/dense-rank semantics, filtered windows, extrema with associated-row values, and named intermediates that are not emitted as analysis columns. |
| X10 | **Schema-scale dependency graph:** a compact CATH slice produces DM, EX, DS, AE, LB, ADSL, ADAE, and ADLB | declarations are intentionally out of dependency order and include shared lookups | Dataset-level topological sorting, column-level dependency extraction from predicates, cycle reporting, caching/reuse, deterministic execution, and useful lineage diagnostics. |
| X11 | **Metadata and artifact contract:** any small end-to-end slice -> CSV and XPT-like metadata manifest | labels, lengths, controlled terminology, origins, dictionary versions, dataset class, and output order are all asserted | Governed metadata vocabulary, inheritance versus overrides, Define-XML-ready lineage, transport constraints, and parity of logical values across physical formats. |

## Priority 3: negative and robustness probes

Positive outputs are not enough to define fail-closed behavior. Add paired
negative fixtures, preferably as small variants of the positive examples.

| ID | Failure to provoke | Required diagnostic |
|---|---|---|
| N01 | two ODM items match one contextual reference | report the contextual keys and matching item records; pass only when an explicit `multiple_matches` policy is present |
| N02 | automatic cross-dataset join has no shared key, a missing key, or duplicate right-side keys | distinguish the three errors and preserve the left row count |
| N03 | duplicate final SDTM/ADaM keys or a missing key value | report the output domain, key variables, and representative offending values |
| N04 | dependency cycle, unresolved variable, or row derivation depending on a column-phase value | report the stable spec paths and cycle path |
| N05 | malformed numeric/date input with and without a conversion handler | fail without a handler; report handler-use counts when handled |
| N06 | duplicate dictionary keys after case folding, missing mapping, and unknown dictionary version | distinguish dictionary-definition errors from data-value errors |
| N07 | invalid SQL predicate and SQL `UNKNOWN` caused by a missing operand | report syntax separately from a valid predicate that evaluates to `UNKNOWN` |
| N08 | ambiguous baseline, closest-visit, or first/last selection tie | fail unless the spec declares a complete deterministic tie-breaker |
| N09 | SDTM-to-ADaM contract drift: required upstream variable absent or wrong type | fail before row execution and list all missing/incompatible input variables |
| N10 | an override, handler, or verification path is declared but never used | execution succeeds but diagnostics report a zero count and stable spec path |

## Implementation status

Priority 1 and Priority 2 are complete. Every entry
below is a positive fixture with committed golden output; no negative fixture
exists yet.

| ID | Fixtures | Result |
|---|---|---|
| C01 | `adam-adsl-treatment-selection`, `adam-adsl-disposition`, `adam-adsl-population-flags` | ADaM side covered; SDTM DM/EX/DS side covered by `sdtm-dm-reference-dates` |
| C02 | `sdtm-lb-findings`, `adam-adlb-bds` | covered as two boundaries |
| C03 | `sdtm-vs-unit-standardization` | conversions expressible; `divide`, `round`, and a literal subtrahend are missing |
| C04 | `adam-adae-treatment-emergent`, `adam-adae-occurrence-flags` | covered |
| C05 | `sdtm-vs-visit-study-day`, `adam-advs-analysis-visit` | covered as two boundaries; `EPOCH` for an unscheduled record needs an interval join |
| C06 | `adam-adsl-mapping`, `sdtm-ae-dictionary-coding`, `adam-adae-severity-override` | inline, external, and override paths covered; governed metadata not covered |
| C07 | `sdtm-suppmh-qualifiers` | reshaping works; the parent-sequence join does not exist |
| C08 | `sdtm-dm-reference-dates` | reference dates derived; no row-wise maximum, no extreme-row selection |
| C09 | `adam-adsl-identifier-parsing`, `adam-adsl-geography-normalization` | covered |
| X01 | `sdtm-lb-multiform` | covered |
| X03 | `adam-adlb-closest-visit` | expressible after all, through a spelled-out absolute value and a negated sort column |
| X04 | `adam-adae-partial-dates` | expressible as string surgery; trailing precision only |
| X09 | `adam-adae-worst-severity` | expressible through a numeric proxy; tied sets cannot be flagged |
| X02 | `sdtm-lb-conditional-compartments` | expressible by row filter; applicability is not declarable and record counts cannot be asserted |
| X05 | `sdtm-ae-effective-transaction` | effective selection works; a removed record cannot be dropped |
| X06 | `adam-adsl-crossover-periods` | period-scoped dates work through aggregate filters; period-scoped values do not |
| X07 | `sdtm-relrec-many-to-many` | expressible with one row template per link slot; group-level rows cannot be keyed |
| X08 | `adam-adrs-composite-response` | expressible; the reason for a value requires writing the predicates twice |
| X10 | `adam-adsl-dependency-order` | column-level graph covered, including predicate-only dependencies; the dataset-level graph cannot be declared |
| X11 | `sdtm-dm-metadata-contract` | metadata is carried but ungoverned; no expected metadata artifact is defined |

Every Priority 1 and Priority 2 case now has a fixture. Not yet implemented:
all of N01-N10.

Three further plan expectations were adjusted by the fixtures. X05 asks whether
transactional ODM must be supported; the fixture shows that selecting an
effective record already works and that only removal is blocked, which narrows
the decision. X10 asks for one slice producing eight datasets, which cannot be
built while one specification derives one dataset, so the fixture covers the
column-level graph only. X11 asks for an asserted metadata manifest; the
fixture declares the metadata but commits no manifest, because inventing its
shape would fix a contract by accident.

Two plan assumptions were corrected by the fixtures. X03 states that `min` and
`max` are insufficient for closest-to-target selection, which is true, but the
selection is expressible with a distance column and a negated ordering column,
so X03 is a cost finding rather than a blocked one. C08 assumes a `_DM_REF`
intermediate dataset is needed to break the reference-date cycle; because one
specification derives one dataset, the cycle does not arise and the real gap is
that cross-specification ordering cannot be declared at all.

## Feature decisions the suite should drive

The examples should be used to decide the following schema additions or rule
clarifications. They should not be implemented ad hoc in only one runtime.

| Area | Current design | Evidence now committed |
|---|---|---|
| Literal-only operands | Arithmetic and banding accept a variable source with literal operands only | Confirmed by `sdtm-vs-unit-standardization` and `adam-adlb-closest-visit`; a target day or conversion factor cannot be data, and the same value must be written twice |
| Missing numeric operations | `divide`, `round`, and absolute value are unregistered, and `subtract` takes no literal | Confirmed by `sdtm-vs-unit-standardization`; `175 LB` standardizes to `79.37866475 kg` and Fahrenheit needs `add` with a negative addend |
| Window operations | Only `row_number` exists; no direction, no `rank`, no `dense_rank`, no filter | Confirmed by `adam-adlb-closest-visit` and `adam-adae-worst-severity`; descending preference needs a negated column and a tied set cannot be flagged |
| Internal intermediates | Multi-step logic requires declared output columns | Confirmed by `adam-adae-partial-dates`, which emits more intermediate than analysis columns, and by `sdtm-vs-visit-study-day`, which emits non-SDTM columns into an SDTM dataset |
| Joins | Automatic many-to-one left join uses intersecting output keys; explicit equality, interval, and non-equi joins are absent | Confirmed by `sdtm-suppmh-qualifiers` for a compound key and `sdtm-vs-visit-study-day` for interval assignment of `EPOCH` |
| Record selection | `min`, `max`, and ordered `multiple_matches` cover only part of first/last/closest/worst logic | Confirmed by `sdtm-dm-reference-dates`; an extreme value and its associated values come from two independent reductions |
| Aggregates | Only `min` and `max` are registered, and both reduce a right side | Confirmed by `sdtm-dm-reference-dates`; a latest-of-several date needs null-guarded `case` branches that grow per candidate |
| Dates and times | `date_diff` is registered; precision, imputation, and comparison under uncertainty are not | Confirmed by `adam-adae-partial-dates`; imputation is regular-expression surgery and an imputed day silently decides treatment emergence |
| Types and conversion | The conversion matrix is unresolved | `sdtm-vs-unit-standardization` proposes a shortest-round-trip float-to-string rule; empty-field ingestion is still assumed, not specified |
| Multi-dataset pipeline | One specification describes one output dataset; no artifact manifest is defined | Confirmed by `sdtm-suppmh-qualifiers` and `sdtm-dm-reference-dates`; cross-specification ordering cannot be declared or cycle-checked |
| Output contract | Output keys and row-wise verifications exist | `sdtm-suppmh-qualifiers` shows no control over output row order and no cross-dataset referential integrity |
| ODM source binding | Contextual item references exist, but exact context keys and zero/multiple-match behavior remain draft | Still open; only `sdtm-lb-multiform` exercises it |
| Row construction | Row templates append filtered driver rows; there is no general distinct/dedup or reusable Findings template | Still open; X01 and `sdtm-suppmh-qualifiers` show the repeated-block cost |
| Metadata and lineage | `label` and free-form string metadata exist, but metadata behavior is not governed | Still open; needs X11 |
| Diagnostics | Handler counts are required, but a common machine-readable diagnostics contract is not defined | Still open; every fixture records expected counts in its README instead |


## Implementation sequence

### Alignment fixture: compact X01 slice

Start with `sdtm-lb-multiform`, a standalone ODM-to-SDTM fixture derived from
the CATH serum, biopsy, saliva, and tape-strip forms. It uses real CATH OIDs but
only two subjects and the records needed to distinguish structural absence,
explicit missingness, zero, a collected nonnumeric result, repeated item
groups, heterogeneous metadata, and deterministic ordering ties.

Use this fixture to agree on difficulty, directory layout, golden outputs,
README evidence, handler counts, and verification reporting. Keep the existing
`adam-adlb-bds` fixture as the separate SDTM-to-ADaM assessment boundary.

### Milestone A: executable common spine

Implement C01, C02, C04, and C05, plus N01-N05 and N09. Together they create a
minimal but real path from ODM through DM/EX/DS/AE/LB into ADSL/ADAE/ADLB. Do
not use `function` to hide missing portable operations; a blocked probe is a
more useful result.

Exit criteria:

- each fixture validates against the schema;
- exact SDTM and ADaM outputs are committed;
- exact errors and handler counts are committed for negative variants;
- R and Python agree on values, row order, types, and failures;
- every behavior is linked to an existing rule or a clearly named rule gap.

### Milestone B: CATH challenge slice

Implement X01-X04 and X09 using a very small subset of the CATH ODM: three
subjects, the relevant baseline/Day-21/unscheduled contexts, and only the item
definitions required by the fixture. Preserve the actual CATH OIDs and repeat
keys so the test exercises real source binding.

Exit criteria:

- heterogeneous CATH biomarker forms produce one conformant LB;
- structurally absent compartments do not produce records;
- visit and baseline selection is deterministic;
- partial dates and selection ties have explicit, portable outcomes or errors;
- the resulting ADLB and ADAE support the variables consumed by the CATH ADaM
  programs.

### Milestone C: advanced trial designs and governance

Implement C06-C09 and X05-X11. Use these to settle transactional ODM scope,
explicit and interval joins, internal variables, governed metadata, and the
multi-output pipeline manifest.

Exit criteria:

- cross-domain and period-aware joins cannot silently change row count;
- metadata and lineage have a machine-readable expected artifact;
- contract drift is detected before derivation starts;
- the larger dependency graph has deterministic execution and cycle errors.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
fixture needs it, a negative or edge fixture fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
fixtures should become closed, documented expressions instead.
