# Benchmark sources for new examples

This file catalogues the [pharmaverse](https://github.com/pharmaverse)
repositories worth mining for new examples, what each one supplies, and which
entry in [`plan.md`](plan.md) it speaks to. It ranks sources; it does not
record findings. A finding an extracted example exposes belongs in `plan.md`,
and the example itself in the index in [`README.md`](README.md).

A source earns a place here by supplying at least one of three things:

- **a stated derivation** whose semantics are written down well enough to
  reproduce without reading an implementation;
- **an input and its exact output**, small enough to trim into `input/*.csv`
  and `expected/*.csv` without inventing values;
- **a rejection**, a documented condition under which the derivation refuses
  to produce a value, which is what a `negative-` example needs.

A source supplying only the first is a reading list. A source supplying the
second is extractable today.

## Licensing

Every repository below is Apache-2.0 except `metatools`, `admiraldiscovery`,
and `metacore`, which are MIT. Both permit derivative work with attribution.
This repository declares no license of its own, so the terms an extracted
example carries are undecided. Settle that before the first extraction lands.

Data trimmed from `pharmaversesdtm` is partly sourced from the
[CDISC pilot project](https://github.com/cdisc-org/sdtm-adam-pilot-project)
and partly constructed by the admiral team; the per-dataset provenance is in
that package's reference pages.

## Tier 1: extract from these first

### `pharmaverse/admiral`

The reference implementation of ADaM derivations and already this suite's
benchmark. Apache-2.0, 148 exported functions.

What makes it extractable is the test suite rather than the source: 1058
`test_that` blocks across 82 files, of which 70 build the expected output as
an inline `tribble` literal beside the input. That is the same contract this
suite holds -- a small input, an exact output, and a named condition -- so a
test converts to an example directory with no interpretation of R semantics.
143 roxygen `@examples` blocks are a second, smaller seam of the same shape,
and 13 `inst/templates/ad_*.R` scripts show how the derivations compose into
one dataset.

Its 20 vignettes speak directly to open work:

| Vignette | Speaks to |
|---|---|
| `visits_periods.Rmd` | gaps 2 and 7, the period and epoch structure |
| `imputation.Rmd` | gap 17, what a partial date may be completed to |
| `generic.Rmd` | gaps 11 and 12, joined and summarized matches |
| `higher_order.Rmd` | gap 1, varying a derivation's arguments by filter |
| `lab_grading.Rmd` | gap 1, the criteria-table form already surveyed |
| `queries_dataset.Rmd` | gap 7, how many query slots a study needs |
| `occds.Rmd`, `bds_finding.Rmd`, `bds_tte.Rmd` | the dataset classes the suite already covers, at study scale |
| `pk_adnca.Rmd`, `questionnaires.Rmd` | ADPC and ADQS, largely uncovered here |

Functions whose behavior no example currently states, grouped by the gap they
would supply evidence for:

- **gap 11, row-relative matching.** `derive_vars_joined`, `filter_joined`,
  `derive_var_joined_exist_flag`, `filter_relative`.
- **gap 12, a reduction over a reduction.** `derive_vars_joined_summary`,
  `derive_var_merged_summary`, `derive_summary_records`.
- **gap 16, row construction that is not append-only.**
  `create_single_dose_dataset`, `derive_expected_records`,
  `derive_locf_records`.
- **gap 15, records becoming fields.** `derive_vars_transposed`.
- **gap 9, a value and the reason for it.** `derive_extreme_event` with
  `event()` and `event_joined()`, which carry the source record's identity
  alongside the value it won with.
- **gap 2, the irregular interval join.** `create_period_dataset`,
  `derive_vars_period`, `derive_var_ontrtfl`.

### `pharmaverse/admiraldiscovery`

MIT. Not example material: a coverage ledger. `inst/admiral-lookup-book.csv`
holds 480 rows of dataset, variable, and the function that derives it, with a
documentation link for each. Read as a backlog it says which ADaM variables
have a published derivation and, against the index in `README.md`, which of
them this suite has never stated.

By that reading the largest uncovered areas are ADPC (43 rows), ADPPK (34),
ADMH (25), ADEG (23), and ADCM (19), none of which this suite touches at all.

### `pharmaverse/sdtm.oak`

Apache-2.0. A raw-to-SDTM transformation engine, and the only pharmaverse
source that covers the half of the pipeline this suite exercises with 15
examples against 39 for ADaM.

It is a peer of `spec.yaml` rather than a library: `inst/raw_data/`
`cm_sdtm_oak_spec.csv` is a machine-readable raw-to-SDTM mapping specification
of 47 columns, holding the mapping algorithm, its sub-algorithm, a condition
in decomposed operator form, merge and unduplication keys, and the controlled
terminology code per target variable. `inst/ct/ct-01-cm.csv` is the paired
terminology specification and `inst/spec/suppqual_spec.csv` the supplemental
qualifier one. Comparing what those columns govern against `schema.yaml` is a
direct test of whether this schema can state a mapping another engine already
executes.

136 `test_that` blocks, 13 files with `tribble` literals, and six vignettes.
`iso_8601.Rmd` is the closest published treatment of what T2 calls precision:
it converts partial and malformed collected dates and reports what it could
not parse, which is gaps 3 and 4 stated as an implementation.

## Tier 2: data and pipeline shape

### `pharmaverse/pharmaversesdtm`

Apache-2.0, 64 datasets, no code. The input side for domains this suite has
never read: `rs_onco*` in six flavors, `tr_onco`, `tu_onco`, `qs*`, `pc`,
`pp`, `eg`, `mb`, `be`, `is*`, `nv_neuro`, `face_vaccine`, `sv`, `ts`, and
eleven `supp*` datasets. Realistic rather than minimal, so a dataset trims
into an example input rather than becoming one.

### `pharmaverse/pharmaverseadam`

Apache-2.0, 30 datasets, no code. The admiral-produced ADaM outputs for those
SDTM inputs. Useful for cross-checking a value derived by hand; poor as golden
output, because each is a whole study rather than the few rows an example
asserts.

### `pharmaverse/pharmaverseraw`

Apache-2.0, five datasets (`ae_raw`, `dm_raw`, `ds_raw`, `ec_raw`, `vs_raw`)
in collected EDC shape. This is the shape `odm.csv` projects and the suite has
one example reading it, so this is the thinnest covered area against an
available source.

### `pharmaverse/examples`

Apache-2.0. End-to-end Quarto workflows: SDTM `dm`, `vs`, `ae`; ADaM `adsl`,
`advs`, `adae`, `adtte`, `adrs`, `adpc`, `adppk`, `ader`. Its
`metadata/*.xlsx` specifications and `metadata/sdtm_ct.csv` are a worked
example of the governed metadata T4 has no artifact for.

Its value is the pipeline rather than any single derivation: it shows one
dataset consumed by the next, which is the multi-dataset contract of gap 5.

## Tier 3: therapeutic-area extensions

Each is Apache-2.0 and follows admiral's testing conventions, so the same
`tribble` extraction applies at smaller volume.

| Repository | Exports / tests | Why it is worth reading |
|---|---|---|
| `admiralonco` | 30 / 34 | Eleven vignettes covering RECIST, iRECIST, PCWG3, IMWG, GCIG, and lymphoma. Confirmed response, clinical benefit, and progression carry the source-record traceability of gap 9, and the nadir comparison is gap 12 |
| `admiralvaccine` | 11 / 35 | ADCE, ADFACE, ADIS. Severity graded from a measured diameter and fever records built where none were collected, which is the row-creation half of gap 16 |
| `admiralophtha` | 5 / 8 | Study eye and affected eye, ETDRS-to-logMAR conversion, criterion flags. Small and clean; laterality is a grain this suite has never carried |
| `admiralpeds` | 3 / 39 | Growth parameters interpolated against 14 WHO and CDC reference tables. The keyed reference table T1 rejected as `cut_from`, at a scale that tests whether the rejection holds |
| `admiralneuro` | 2 / 30 | Centiloid and percentile computations. Two exports; low yield |
| `admiralmetabolic` | 2 / 13 | Waist-to-hip and waist-to-height ratios. Two exports; low yield |

## Tier 4: rules, metadata, and record selection

### `pharmaverse/sdtmchecks`

Apache-2.0, default branch `devel`. 109 `check_*` functions, each a
cross-domain SDTM consistency rule stated over real variables: a death date
that disagrees with the outcome, a start date after an end date, a
disposition that no adverse event supports. This is the densest available
source for the `verifications` vocabulary and for negative examples, because
each check names a condition and the rows that violate it.

### `pharmaverse/metatools`

MIT, 22 exports. Two seams. `build_qnam`, `make_supp_qual`, and
`combine_supp` are the supplemental-qualifier round trip that gap 15 leaves
half-built here, `combine_supp` being the return direction no example
performs. `check_ct_col`, `check_variables`, `check_unique_keys`, and
`drop_unspec_vars` check a dataset against a declared specification, which is
the conformance half of T4.

### `pharmaverse/datacutr`

Apache-2.0, 10 exports, seven test datasets. Applies a cut date across a
study: date cuts, patient cuts, a special case for DM, and imputation of a
partial `DCUTDTC` bounded by the cut. The bound is gap 17's third item, and
removing records that fall after the cut is gap 8.

### `pharmaverse/admiral-adamig-wg`

No code; an issue tracker of ADaM Implementation Guide ambiguities, such as
traceability under multiple imputation, criterion variables with more than
two responses, and copying values onto a new record. Each open question is a
case where two readings are defensible, which is what a `negative-` example
states when it asks for an explicit policy rather than a correction.

## Adjacent, outside the pharmaverse organization

`metacore` and `xportr` (both `atorus-research`, MIT) are the pharmaverse
metadata stack: `metacore` reads a define.xml or a specification workbook into
a governed object carrying origin, length, codelist, core, and display format,
and `xportr` applies and enforces them on the way to a transport file. That is
the vocabulary T4 says has to be named, already named, so read `metacore`
before designing the governed metadata block.

## Reading order

1. `admiraldiscovery` first, to size the gap between the published derivation
   catalogue and the index in `README.md`.
2. `admiral` tests for the extractable pairs, taking the gap-targeted
   functions above before the uncovered domains.
3. `sdtm.oak` next, both for SDTM coverage and to compare its specification
   columns against `schema.yaml`.
4. `sdtmchecks` for verification and negative coverage, which is where this
   suite is proportionally strongest and therefore most likely to find a
   condition it has not named.
5. The therapeutic-area extensions last, weighted by the gap each speaks to
   rather than by size.
