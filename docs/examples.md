---
title: Example gallery
---

# The example gallery

<!-- BEGIN GENERATED: docs-nav -->
> **YAMAA docs:** [Index](index.md) | [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md) | **Example gallery**
<!-- END GENERATED: docs-nav -->

> **Read this if** you want the complete list: which example answers which
> question, and which failure each rejected specification pins down.
>
> For how one example is put together and how to read it, see the
> [examples walkthrough](yaml-examples-walkthrough.md). For the concepts the
> specifications use, see [Schema concepts](schema-concepts.md).

---

Every entry links to its directory on GitHub, where the README states what the
output means, `spec.yaml` states how it is produced, and `expected/` holds
either the artifact an implementation must reproduce or the error it must
raise. Each description here repeats its README title, so the two cannot drift
apart.

This page is generated from the directories in `yaml/examples/`. Adding an
example means running
[`generate_example_gallery.py`](https://github.com/elong0527/yamaa/blob/main/.github/workflows/generate_example_gallery.py),
not editing this list.

<!-- BEGIN GENERATED: example-gallery -->
The suite holds **167 examples**: 91 that must run, and 76 that must be
rejected.

| Group | Count | What it is |
|---|---|---|
| [ADaM](#adam) | 72 | analysis datasets derived from collected data |
| [SDTM](#sdtm) | 18 | tabulation datasets built from collected data |
| [ODM](#odm) | 1 | how a collected item resolves to a value |
| [Rejected](#negative-examples) | 76 | specifications the design must reject, with the exact error |

## Positive examples

Every one of these runs to completion and produces the artifact its `expected/`
directory holds, byte for byte.

### ADaM

| Example | Derives |
|---|---|
| [`adam-adae-death-outcome`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-death-outcome) | carry each subject's death onto every event |
| [`adam-adae-occurrence-flags`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-occurrence-flags) | flag the first occurrence at three levels |
| [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-partial-dates) | impute partial dates |
| [`adam-adae-post-dose-onset`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-post-dose-onset) | classify an event by the moment it started |
| [`adam-adae-post-reference-event`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-post-reference-event) | flag an event after a specific reference event |
| [`adam-adae-protocol-review-window`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-protocol-review-window) | flag adverse events for protocol review |
| [`adam-adae-query-flags`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-query-flags) | record which queries a coded event belongs to |
| [`adam-adae-review-order`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-review-order) | present a subject's events in medical-review order |
| [`adam-adae-serious-event-listing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-serious-event-listing) | list the serious adverse events |
| [`adam-adae-serious-event-sequence`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-serious-event-sequence) | number a subject's serious events in onset order |
| [`adam-adae-severity-override`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-severity-override) | apply an approved severity correction |
| [`adam-adae-severity-rank`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-severity-rank) | rank a subject's events by severity |
| [`adam-adae-string-handlers`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-string-handlers) | clean text and handle invalid IDs |
| [`adam-adae-treatment-emergent`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-treatment-emergent) | classify an event as treatment-emergent |
| [`adam-adae-worst-severity`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-worst-severity) | flag the worst-severity event per preferred term |
| [`adam-adce-worst-toxicity-grade`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adce-worst-toxicity-grade) | flag the subject's worst-grade event |
| [`adam-adcm-on-treatment-flag`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adcm-on-treatment-flag) | flag a medication during treatment |
| [`adam-adeg-bazett-qtc`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adeg-bazett-qtc) | derive a Bazett-corrected QT parameter |
| [`adam-adeg-fridericia-qtc`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adeg-fridericia-qtc) | derive a Fridericia-corrected QT parameter |
| [`adam-adeg-rr-interval`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adeg-rr-interval) | derive an RR interval |
| [`adam-adex-cumulative-dose`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-cumulative-dose) | summarize cumulative exposure |
| [`adam-adex-dose-reduction-flag`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-dose-reduction-flag) | derive a dose reduction flag |
| [`adam-adex-uncollected-exposure`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-uncollected-exposure) | tell an uncollected dose from an absent administration |
| [`adam-adlb-absolute-wbc-differential`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-absolute-wbc-differential) | derive absolute WBC differentials |
| [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds) | build a BDS dataset with baseline and change |
| [`adam-adlb-closest-visit`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-closest-visit) | select the record closest to a window's target day |
| [`adam-adlb-mean`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-mean) | calculate each subject's mean result |
| [`adam-adlb-reported-precision`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-reported-precision) | report a result against the lower limit of normal |
| [`adam-adlb-shift-and-criteria`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-shift-and-criteria) | classify a result, its shift from baseline, and one criterion |
| [`adam-adlb-standardized-result`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-standardized-result) | carry standardized results into analysis |
| [`adam-adoe-study-eye`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adoe-study-eye) | tell the study eye from the fellow eye |
| [`adam-adqs-subscale-score`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adqs-subscale-score) | score a questionnaire subscale from its item records |
| [`adam-adrs-best-overall-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-best-overall-response) | select the best overall response |
| [`adam-adrs-best-response-selection`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-best-response-selection) | prepare assessments for best overall response |
| [`adam-adrs-composite-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-composite-response) | combine efficacy, safety, and discontinuation into one response |
| [`adam-adrs-confirmed-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-confirmed-response) | confirm an objective response |
| [`adam-adrs-measurable-disease`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-measurable-disease) | derive measurable disease at baseline |
| [`adam-adrs-overall-response-records`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-overall-response-records) | prepare the overall response records an endpoint reads |
| [`adam-adsl-analysis-age`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-analysis-age) | analysis age |
| [`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-compute) | compute BMI from height and weight |
| [`adam-adsl-bmi-function`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-function) | compute BMI by calling a routine the project supplies |
| [`adam-adsl-completion-flag`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-completion-flag) | flag the subjects who completed the study |
| [`adam-adsl-crossover-periods`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-crossover-periods) | derive period-scoped treatments and dates across a washout |
| [`adam-adsl-dependency-order`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-dependency-order) | derive a chain of population flags |
| [`adam-adsl-disposition`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-disposition) | select the final subject disposition from DS |
| [`adam-adsl-dose-adjustment-flag`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-dose-adjustment-flag) | derive a dose adjustment flag from multiple sources |
| [`adam-adsl-geography-normalization`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-geography-normalization) | normalize collected country and group it into a region |
| [`adam-adsl-identifier-parsing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-identifier-parsing) | parse the site from USUBJID with a collected fallback |
| [`adam-adsl-investigator-comment`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-investigator-comment) | keep an investigator comment exactly as collected |
| [`adam-adsl-last-alive-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-last-alive-date) | derive the last known alive date from multiple sources |
| [`adam-adsl-mapping`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-mapping) | translate collected values into a standard vocabulary |
| [`adam-adsl-new-anticancer-therapy-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-new-anticancer-therapy-date) | date the subject started new anti-cancer therapy |
| [`adam-adsl-non-finite-values`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-non-finite-values) | normalize non-finite numeric values to missing |
| [`adam-adsl-population-flags`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-population-flags) | derive the safety and intent-to-treat flags |
| [`adam-adsl-portable-text`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-portable-text) | preserve and compare international text predictably |
| [`adam-adsl-randomization-timing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-randomization-timing) | record randomization timing |
| [`adam-adsl-rescue-medication`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-rescue-medication) | select the first rescue medication |
| [`adam-adsl-treatment-selection`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-treatment-selection) | select actual treatment and its duration from EX |
| [`adam-adtr-current-nadir`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtr-current-nadir) | derive the current nadir |
| [`adam-adtr-sum-of-target-diameters`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtr-sum-of-target-diameters) | sum the target lesion diameters at each assessment |
| [`adam-adtte-duration-of-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-duration-of-response) | derive the duration of a response |
| [`adam-adtte-first-adverse-event`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-first-adverse-event) | derive the time to first adverse event |
| [`adam-adtte-overall-survival`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-overall-survival) | derive overall survival |
| [`adam-adtte-progression-free-survival`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-progression-free-survival) | derive progression-free survival |
| [`adam-advs-analysis-visit`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-analysis-visit) | assign records to analysis windows |
| [`adam-advs-analysis-window-table`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-analysis-window-table) | assign analysis windows from the study's window table |
| [`adam-advs-body-mass-index`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-body-mass-index) | derive body mass index |
| [`adam-advs-body-surface-area`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-body-surface-area) | derive a body surface area parameter |
| [`adam-advs-growth-percentile`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-growth-percentile) | express a measurement as a growth percentile |
| [`adam-advs-mean-arterial-pressure`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-mean-arterial-pressure) | derive mean arterial pressure |
| [`adam-advs-once-measured-carry-forward`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-once-measured-carry-forward) | carry forward a once-measured characteristic |
| [`adam-advs-prior-character-result`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-prior-character-result) | retain the latest earlier character result |

### SDTM

| Example | Derives |
|---|---|
| [`sdtm-ae-dictionary-coding`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ae-dictionary-coding) | code reported terms against a medical dictionary |
| [`sdtm-ae-effective-transaction`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ae-effective-transaction) | take the effective state of a record from a transaction log |
| [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-basic) | build one subject record from collected data |
| [`sdtm-dm-metadata-contract`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-metadata-contract) | declare the metadata a submission needs |
| [`sdtm-dm-reference-dates`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-reference-dates) | derive the reference dates from EX, DS, and AE |
| [`sdtm-ds-disposition-sequence`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ds-disposition-sequence) | number each subject's disposition records in date order |
| [`sdtm-ex-combination-regimen`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ex-combination-regimen) | represent a combination regimen |
| [`sdtm-fa-fever-occurrence`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-fa-fever-occurrence) | fever occurrence |
| [`sdtm-lb-conditional-compartments`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-conditional-compartments) | tell an inapplicable compartment from an uncollected sample |
| [`sdtm-lb-ctcae-grading`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-ctcae-grading) | assign toxicity grades |
| [`sdtm-lb-findings`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-findings) | build one record per collected lab result |
| [`sdtm-lb-multiform`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-multiform) | consolidate four collection forms into one dataset |
| [`sdtm-lb-reference-range-indicator`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-reference-range-indicator) | apply external reference ranges |
| [`sdtm-relrec-many-to-many`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-relrec-many-to-many) | record relationships between events and medications |
| [`sdtm-suppmh-parent-linkage`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-suppmh-parent-linkage) | link qualifiers collected on their own form to a parent record |
| [`sdtm-suppmh-qualifiers`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-suppmh-qualifiers) | reshape extra qualifiers into supplemental records |
| [`sdtm-vs-unit-standardization`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-vs-unit-standardization) | standardize collected results into the study's units |
| [`sdtm-vs-visit-study-day`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-vs-visit-study-day) | attach visit metadata and study day to a result |

### ODM

| Example | Derives |
|---|---|
| [`odm-form-scoped-item-resolution`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/odm-form-scoped-item-resolution) | resolve items within their collection form |

## Negative examples

Each of these must fail, and `expected/error.yaml` pins exactly how. They are
grouped by the phase that rejects the run, so an implementation can close one
phase at a time.

### Rejected at `validation`

43 examples, rejecting the specification itself, before any data is read.

| Example | Rejects |
|---|---|
| [`negative-adae-review-condition-arithmetic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adae-review-condition-arithmetic) | reject a review flag whose condition performs arithmetic |
| [`negative-adae-review-text-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adae-review-text-date) | reject a review flag that compares a date with text |
| [`negative-adae-review-unknown-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adae-review-unknown-date) | reject a review flag that names an unavailable date |
| [`negative-adex-relative-dose-intensity`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adex-relative-dose-intensity) | reject a dose intensity measured against a per-record plan |
| [`negative-adlb-computed-parameter`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adlb-computed-parameter) | reject a parameter computed from the dataset being built |
| [`negative-adsl-cyclic-parent`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-cyclic-parent) | reject a circular chain of shared definitions |
| [`negative-adsl-inherited-output`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-inherited-output) | reject an inherited artifact layout |
| [`negative-adsl-invalid-parent-clear`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-invalid-parent-clear) | reject removal of a required variable property |
| [`negative-adsl-parent-version-mismatch`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-parent-version-mismatch) | reject shared definitions from another language version |
| [`negative-adsl-randomization-date-retyped`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-randomization-date-retyped) | reject a randomization date described twice |
| [`negative-adsl-remote-parent`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-remote-parent) | reject shared definitions from a remote location |
| [`negative-adsl-subject-reference`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-subject-reference) | reject a malformed subject reference |
| [`negative-column-type-unknown`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-column-type-unknown) | reject an analysis value with an ambiguous numeric type |
| [`negative-compute-aggregate-function`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-aggregate-function) | reject a total written as a formula |
| [`negative-compute-comparison-operator`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-comparison-operator) | reject an above-range flag written as a formula |
| [`negative-compute-qualified-identifier`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-qualified-identifier) | reject a doubled dose read straight from exposure |
| [`negative-dataset-path-absolute`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-absolute) | reject reference limits named by a machine location |
| [`negative-dataset-path-directory`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-directory) | reject reference limits that name a folder |
| [`negative-dataset-path-missing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-missing) | reject reference limits the study does not hold |
| [`negative-dataset-path-parent-escape`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-parent-escape) | reject reference limits stored above the study |
| [`negative-dataset-path-symlink`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-symlink) | reject reference limits reached through a stand-in name |
| [`negative-dataset-path-url`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-dataset-path-url) | reject reference limits named by a web address |
| [`negative-date-impute-month-out-of-range`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-impute-month-out-of-range) | reject a start date completed with no month of the year |
| [`negative-date-impute-unknown-day-rule`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-impute-unknown-day-rule) | reject a start date completed with an unrecognised day |
| [`negative-function-contract-mismatch`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-function-contract-mismatch) | reject an unavailable project-routine contract |
| [`negative-greatest-incomparable-sources`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-greatest-incomparable-sources) | reject a last-known-alive date taken from a day number |
| [`negative-group-count-without-id`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-group-count-without-id) | reject an unnamed baseline-count rule |
| [`negative-keys-internal-column`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-keys-internal-column) | reject a site-scoped subject identity |
| [`negative-mapping-case-fold-collision`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-case-fold-collision) | reject a smoking flag whose dictionary answers twice |
| [`negative-mapping-from-key-length-mismatch`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-from-key-length-mismatch) | reject a reference range chosen by an unpaired key |
| [`negative-output-order-repeated-term`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-output-order-repeated-term) | reject an order that places one value twice |
| [`negative-output-order-unknown-column`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-output-order-unknown-column) | reject a submission order over a value the dataset does not carry |
| [`negative-record-lookup-id-collision`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-id-collision) | reject a first treatment named after its own source |
| [`negative-record-lookup-incomparable-range`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-incomparable-range) | reject an epoch range with incomparable endpoints |
| [`negative-record-lookup-unordered-keep`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-unordered-keep) | reject a treatment ordered but not chosen |
| [`negative-record-lookup-unpaired-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-unpaired-key) | reject a reference limit matched against nothing |
| [`negative-row-value-self-reference`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-row-value-self-reference) | reject a weight carried forward from a carried-forward weight |
| [`negative-row-value-zero-offset`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-row-value-zero-offset) | reject a previous weight that names no earlier visit |
| [`negative-source-output-self-reference`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-source-output-self-reference) | reject a parameter that reads the dataset it is part of |
| [`negative-sum-non-numeric-source`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-sum-non-numeric-source) | reject a severity burden totalled from severity words |
| [`negative-to-date-date-source`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-to-date-date-source) | reject extracting a date from a date |
| [`negative-types-unknown-field`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-types-unknown-field) | reject a total over a field the source does not have |
| [`negative-variable-nested-expression`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-variable-nested-expression) | reject an uppercased country chosen inside the same step |

### Rejected at `ingest`

1 example, rejecting a stored value, against the type its field carries.

| Example | Rejects |
|---|---|
| [`negative-ingest-unparseable-field`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-ingest-unparseable-field) | reject a dose recorded with its unit |

### Rejected at `row_construction`

1 example, rejecting evaluating a row template.

| Example | Rejects |
|---|---|
| [`negative-adlb-absolute-wbc-duplicate`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adlb-absolute-wbc-duplicate) | reject duplicate WBC inputs for an absolute differential |

### Rejected at `derivation`

5 examples, rejecting evaluating a column's expression over a row.

| Example | Rejects |
|---|---|
| [`negative-baseline-flag-tied-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-baseline-flag-tied-date) | reject a baseline chosen between two same-day results |
| [`negative-compute-division-by-zero`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-division-by-zero) | reject a percent change from a zero baseline |
| [`negative-compute-integer-overflow`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-integer-overflow) | reject a cell total larger than the counter can hold |
| [`negative-compute-ln-of-zero`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-ln-of-zero) | reject a log result from an undetectable value |
| [`negative-compute-sqrt-of-negative`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-compute-sqrt-of-negative) | reject a body surface area from a negative weight |

### Rejected at `output`

2 examples, rejecting output identity, once every column holds its final value.

| Example | Rejects |
|---|---|
| [`negative-keys-missing-value`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-keys-missing-value) | reject a record that no analysis visit identifies |
| [`negative-output-duplicate-subject`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-output-duplicate-subject) | reject a repeated demographics record |

### Rejected at `verification`

7 examples, rejecting a declared assertion.

| Example | Rejects |
|---|---|
| [`negative-adam-adeg-pre-existing-rrr`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adam-adeg-pre-existing-rrr) | reject a collected RR interval |
| [`negative-adam-adsl-stratification-reconciliation`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adam-adsl-stratification-reconciliation) | reconcile randomization strata |
| [`negative-adex-single-dose-expansion`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adex-single-dose-expansion) | reject one record per administration built from an aggregate dose |
| [`negative-adlb-multiple-baseline-records`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adlb-multiple-baseline-records) | reject a subject with two baseline records for one parameter |
| [`negative-adrs-partial-response-after-complete-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adrs-partial-response-after-complete-response) | reject a partial response recorded after a complete response |
| [`negative-usubjid-exceeds-length`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-usubjid-exceeds-length) | reject a subject identifier longer than the study permits |
| [`negative-verification-implausible-age`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-verification-implausible-age) | reject an implausible age |

### Rejected at `join`

6 examples, rejecting an unresolved multiple match.

| Example | Rejects |
|---|---|
| [`negative-advs-overlapping-analysis-windows`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-advs-overlapping-analysis-windows) | reject overlapping analysis windows |
| [`negative-query-slot-overflow`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-query-slot-overflow) | reject an event belonging to more queries than it has places |
| [`negative-record-lookup-incomplete-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-incomplete-key) | reject a reference limit chosen without a sex |
| [`negative-record-lookup-unmatched-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-unmatched-key) | reject a result with no reference range |
| [`negative-record-lookup-unordered-choice`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-record-lookup-unordered-choice) | reject a treatment and dose taken from an unchosen record |
| [`negative-source-duplicate-right-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-source-duplicate-right-key) | reject duplicate subject enrichment |

### Rejected at `mapping`

4 examples, rejecting a missing or unmapped lookup input.

| Example | Rejects |
|---|---|
| [`negative-mapping-from-duplicate-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-from-duplicate-key) | reject a reference range stated twice |
| [`negative-mapping-from-partial-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-from-partial-key) | reject a reference range chosen without a sex |
| [`negative-mapping-from-unmapped-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-from-unmapped-key) | reject a result with no reference range |
| [`negative-mapping-unmapped-value`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-mapping-unmapped-value) | reject an unmapped response |

### Rejected at `impute`

3 examples, rejecting a missing or unusable partial-date input.

| Example | Rejects |
|---|---|
| [`negative-date-impute-invalid-source`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-impute-invalid-source) | reject a start date completed from text that is not a date |
| [`negative-date-impute-nonexistent-day`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-impute-nonexistent-day) | reject an end date completed past the end of its month |
| [`negative-date-precision-invalid-source`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-precision-invalid-source) | reject a completeness flag read from text that is not a date |

### Rejected at `convert`

4 examples, rejecting a result that cannot take its declared type.

| Example | Rejects |
|---|---|
| [`negative-conversion-incomplete-date`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-conversion-incomplete-date) | reject an event start date that names no day |
| [`negative-conversion-non-integral`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-conversion-non-integral) | reject a pulse rate recorded between whole beats |
| [`negative-conversion-unparseable-number`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-conversion-unparseable-number) | reject a viral load reported below the assay limit |
| [`negative-datetime-zone-offset`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-datetime-zone-offset) | reject an event start recorded against another clock |
<!-- END GENERATED: example-gallery -->
