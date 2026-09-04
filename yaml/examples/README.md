# Derivation schema examples

These examples exercise `yaml/schema.yaml` with small inputs and exact expected
outputs. They are intended for human review, automated tests, and AI-assisted
implementation. Each holds a specification, its source CSVs, the exact expected
CSV or error, and a README describing what the result means.

Execution behavior is defined by the schema's adjacent operation descriptions
and the shared normative rules in [`../rules/README.md`](../rules/README.md);
dataset declarations, variable references, and ODM contextual lookups by
[R002](../rules/R002-source-binding.md). Example READMEs describe data, not the
specification; [`agents.md`](agents.md) states that contract.

`odm.csv` is a tabular projection of ODM clinical data, not an ODM exchange
document itself. Its fields map to the official
[CDISC ODM 2.0 clinical-data schema](https://github.com/cdisc-org/DataExchange-ODM/blob/main/schema/ODM-clinicaldata.xsd).

New to the suite? Read [`sdtm-dm-basic`](sdtm-dm-basic/) for direct mapping,
[`sdtm-lb-findings`](sdtm-lb-findings/) for row construction, and
[`adam-adlb-bds`](adam-adlb-bds/) for a full BDS derivation, in that order.

## Index

An example that cannot express something records a design finding. The suite
passes when its declared error occurs. Those findings are collected in this
repository's issue tracker, which also carries the schema work they justify.

Expected-failure examples carry `expected/error.yaml`. When rejection happens
after the dataset is completed, an expected CSV records the rows presented to
the failing check. When a missing capability prevents execution, an expected
CSV records the intended artifact once that capability exists.

Every expected-failure README ends with a `How to fix` section that recommends
the safest correction and shows the smallest useful YAML change.

| Example | Derives |
|---|---|
| [`adam-adae-death-outcome`](adam-adae-death-outcome/) | carry each subject's death onto every event |
| [`adam-adae-occurrence-flags`](adam-adae-occurrence-flags/) | flag the first occurrence at three levels |
| [`adam-adae-partial-dates`](adam-adae-partial-dates/) | impute partial dates |
| [`adam-adae-post-dose-onset`](adam-adae-post-dose-onset/) | classify an event by the moment it started |
| [`adam-adae-post-reference-event`](adam-adae-post-reference-event/) | flag an event after a specific reference event |
| [`adam-adae-protocol-review-window`](adam-adae-protocol-review-window/) | flag adverse events for protocol review |
| [`adam-adae-query-flags`](adam-adae-query-flags/) | record which queries a coded event belongs to |
| [`adam-adae-review-order`](adam-adae-review-order/) | present a subject's events in medical-review order |
| [`adam-adae-serious-event-sequence`](adam-adae-serious-event-sequence/) | number a subject's serious events in onset order |
| [`adam-adae-severity-override`](adam-adae-severity-override/) | apply an approved severity correction |
| [`adam-adae-severity-rank`](adam-adae-severity-rank/) | rank a subject's events by severity |
| [`adam-adae-string-handlers`](adam-adae-string-handlers/) | clean text and handle invalid IDs |
| [`adam-adae-treatment-emergent`](adam-adae-treatment-emergent/) | classify an event as treatment-emergent |
| [`adam-adae-worst-severity`](adam-adae-worst-severity/) | flag the worst-severity event per preferred term |
| [`adam-adce-worst-toxicity-grade`](adam-adce-worst-toxicity-grade/) | flag the subject's worst-grade event |
| [`adam-adcm-on-treatment-flag`](adam-adcm-on-treatment-flag/) | flag a medication during treatment |
| [`adam-adeg-bazett-qtc`](adam-adeg-bazett-qtc/) | derive a Bazett-corrected QT parameter |
| [`adam-adeg-fridericia-qtc`](adam-adeg-fridericia-qtc/) | derive a Fridericia-corrected QT parameter |
| [`adam-adeg-rr-interval`](adam-adeg-rr-interval/) | derive an RR interval |
| [`adam-adex-cumulative-dose`](adam-adex-cumulative-dose/) | summarize cumulative exposure |
| [`adam-adex-dose-reduction-flag`](adam-adex-dose-reduction-flag/) | derive a dose reduction flag |
| [`adam-adex-uncollected-exposure`](adam-adex-uncollected-exposure/) | tell an uncollected dose from an absent administration |
| [`adam-adlb-absolute-wbc-differential`](adam-adlb-absolute-wbc-differential/) | derive absolute WBC differentials |
| [`adam-adlb-bds`](adam-adlb-bds/) | build a BDS dataset with baseline and change |
| [`adam-adlb-closest-visit`](adam-adlb-closest-visit/) | select the record closest to a window's target day |
| [`adam-adlb-mean`](adam-adlb-mean/) | calculate each subject's mean result |
| [`adam-adlb-shift-and-criteria`](adam-adlb-shift-and-criteria/) | classify a result, its shift from baseline, and one criterion |
| [`adam-adlb-standardized-result`](adam-adlb-standardized-result/) | carry standardized results into analysis |
| [`adam-adoe-study-eye`](adam-adoe-study-eye/) | tell the study eye from the fellow eye |
| [`adam-adqs-subscale-score`](adam-adqs-subscale-score/) | score a questionnaire subscale from its item records |
| [`adam-adrs-best-overall-response`](adam-adrs-best-overall-response/) | select the best overall response |
| [`adam-adrs-best-response-selection`](adam-adrs-best-response-selection/) | prepare assessments for best overall response |
| [`adam-adrs-composite-response`](adam-adrs-composite-response/) | combine efficacy, safety, and discontinuation into one response |
| [`adam-adrs-confirmed-response`](adam-adrs-confirmed-response/) | confirm an objective response |
| [`adam-adrs-measurable-disease`](adam-adrs-measurable-disease/) | derive measurable disease at baseline |
| [`adam-adrs-overall-response-records`](adam-adrs-overall-response-records/) | prepare the overall response records an endpoint reads |
| [`adam-adsl-analysis-age`](adam-adsl-analysis-age/) | analysis age |
| [`adam-adsl-bmi-compute`](adam-adsl-bmi-compute/) | compute BMI from height and weight |
| [`adam-adsl-bmi-function`](adam-adsl-bmi-function/) | compute BMI by calling a routine the project supplies |
| [`adam-adsl-completion-flag`](adam-adsl-completion-flag/) | flag the subjects who completed the study |
| [`adam-adsl-crossover-periods`](adam-adsl-crossover-periods/) | derive period-scoped treatments and dates across a washout |
| [`adam-adsl-dependency-order`](adam-adsl-dependency-order/) | derive a chain of population flags |
| [`adam-adsl-disposition`](adam-adsl-disposition/) | select the final subject disposition from DS |
| [`adam-adsl-dose-adjustment-flag`](adam-adsl-dose-adjustment-flag/) | derive a dose adjustment flag from multiple sources |
| [`adam-adsl-geography-normalization`](adam-adsl-geography-normalization/) | normalize collected country and group it into a region |
| [`adam-adsl-identifier-parsing`](adam-adsl-identifier-parsing/) | parse the site from USUBJID with a collected fallback |
| [`adam-adsl-last-alive-date`](adam-adsl-last-alive-date/) | derive the last known alive date from multiple sources |
| [`adam-adsl-mapping`](adam-adsl-mapping/) | translate collected values into a standard vocabulary |
| [`adam-adsl-new-anticancer-therapy-date`](adam-adsl-new-anticancer-therapy-date/) | date the subject started new anti-cancer therapy |
| [`adam-adsl-non-finite-values`](adam-adsl-non-finite-values/) | normalize non-finite numeric values to missing |
| [`adam-adsl-population-flags`](adam-adsl-population-flags/) | derive the safety and intent-to-treat flags |
| [`adam-adsl-randomization-timing`](adam-adsl-randomization-timing/) | record randomization timing |
| [`adam-adsl-rescue-medication`](adam-adsl-rescue-medication/) | select the first rescue medication |
| [`adam-adsl-treatment-selection`](adam-adsl-treatment-selection/) | select actual treatment and its duration from EX |
| [`adam-adtr-current-nadir`](adam-adtr-current-nadir/) | derive the current nadir |
| [`adam-adtr-sum-of-target-diameters`](adam-adtr-sum-of-target-diameters/) | sum the target lesion diameters at each assessment |
| [`adam-adtte-duration-of-response`](adam-adtte-duration-of-response/) | derive the duration of a response |
| [`adam-adtte-progression-free-survival`](adam-adtte-progression-free-survival/) | derive progression-free survival |
| [`adam-advs-analysis-visit`](adam-advs-analysis-visit/) | assign records to analysis windows |
| [`adam-advs-analysis-window-table`](adam-advs-analysis-window-table/) | assign analysis windows from the study's window table |
| [`adam-advs-body-mass-index`](adam-advs-body-mass-index/) | derive body mass index |
| [`adam-advs-body-surface-area`](adam-advs-body-surface-area/) | derive a body surface area parameter |
| [`adam-advs-growth-percentile`](adam-advs-growth-percentile/) | express a measurement as a growth percentile |
| [`adam-advs-mean-arterial-pressure`](adam-advs-mean-arterial-pressure/) | derive mean arterial pressure |
| [`adam-advs-once-measured-carry-forward`](adam-advs-once-measured-carry-forward/) | carry forward a once-measured characteristic |
| [`negative-adae-review-condition-arithmetic`](negative-adae-review-condition-arithmetic/) | reject a review flag whose condition performs arithmetic |
| [`negative-adae-review-text-date`](negative-adae-review-text-date/) | reject a review flag that compares a date with text |
| [`negative-adae-review-unknown-date`](negative-adae-review-unknown-date/) | reject a review flag that names an unavailable date |
| [`negative-adam-adeg-pre-existing-rrr`](negative-adam-adeg-pre-existing-rrr/) | reject a collected RR interval |
| [`negative-adam-adsl-stratification-reconciliation`](negative-adam-adsl-stratification-reconciliation/) | reconcile randomization strata |
| [`negative-adex-relative-dose-intensity`](negative-adex-relative-dose-intensity/) | reject a dose intensity measured against a per-record plan |
| [`negative-adex-single-dose-expansion`](negative-adex-single-dose-expansion/) | reject one record per administration built from an aggregate dose |
| [`negative-adlb-absolute-wbc-duplicate`](negative-adlb-absolute-wbc-duplicate/) | reject duplicate WBC inputs for an absolute differential |
| [`negative-adlb-computed-parameter`](negative-adlb-computed-parameter/) | reject a parameter computed from the dataset being built |
| [`negative-adlb-multiple-baseline-records`](negative-adlb-multiple-baseline-records/) | reject a subject with two baseline records for one parameter |
| [`negative-adrs-partial-response-after-complete-response`](negative-adrs-partial-response-after-complete-response/) | reject a partial response recorded after a complete response |
| [`negative-adsl-cyclic-parent`](negative-adsl-cyclic-parent/) | reject a circular chain of shared definitions |
| [`negative-adsl-inherited-output`](negative-adsl-inherited-output/) | reject an inherited artifact layout |
| [`negative-adsl-invalid-parent-clear`](negative-adsl-invalid-parent-clear/) | reject removal of a required variable property |
| [`negative-adsl-parent-version-mismatch`](negative-adsl-parent-version-mismatch/) | reject shared definitions from another language version |
| [`negative-adsl-randomization-date-retyped`](negative-adsl-randomization-date-retyped/) | reject a randomization date described twice |
| [`negative-adsl-remote-parent`](negative-adsl-remote-parent/) | reject shared definitions from a remote location |
| [`negative-adsl-subject-reference`](negative-adsl-subject-reference/) | reject a malformed subject reference |
| [`negative-advs-overlapping-analysis-windows`](negative-advs-overlapping-analysis-windows/) | reject overlapping analysis windows |
| [`negative-baseline-flag-tied-date`](negative-baseline-flag-tied-date/) | reject a baseline chosen between two same-day results |
| [`negative-column-type-unknown`](negative-column-type-unknown/) | reject an analysis value with an ambiguous numeric type |
| [`negative-compute-aggregate-function`](negative-compute-aggregate-function/) | reject a total written as a formula |
| [`negative-compute-comparison-operator`](negative-compute-comparison-operator/) | reject an above-range flag written as a formula |
| [`negative-compute-division-by-zero`](negative-compute-division-by-zero/) | reject a percent change from a zero baseline |
| [`negative-compute-integer-overflow`](negative-compute-integer-overflow/) | reject a cell total larger than the counter can hold |
| [`negative-compute-ln-of-zero`](negative-compute-ln-of-zero/) | reject a log result from an undetectable value |
| [`negative-compute-qualified-identifier`](negative-compute-qualified-identifier/) | reject a doubled dose read straight from exposure |
| [`negative-compute-sqrt-of-negative`](negative-compute-sqrt-of-negative/) | reject a body surface area from a negative weight |
| [`negative-conversion-incomplete-date`](negative-conversion-incomplete-date/) | reject an event start date that names no day |
| [`negative-conversion-non-integral`](negative-conversion-non-integral/) | reject a pulse rate recorded between whole beats |
| [`negative-conversion-unparseable-number`](negative-conversion-unparseable-number/) | reject a viral load reported below the assay limit |
| [`negative-date-impute-invalid-source`](negative-date-impute-invalid-source/) | reject a start date completed from text that is not a date |
| [`negative-date-impute-month-out-of-range`](negative-date-impute-month-out-of-range/) | reject a start date completed with no month of the year |
| [`negative-date-impute-nonexistent-day`](negative-date-impute-nonexistent-day/) | reject an end date completed past the end of its month |
| [`negative-date-impute-unknown-day-rule`](negative-date-impute-unknown-day-rule/) | reject a start date completed with an unrecognised day |
| [`negative-date-precision-invalid-source`](negative-date-precision-invalid-source/) | reject a completeness flag read from text that is not a date |
| [`negative-datetime-zone-offset`](negative-datetime-zone-offset/) | reject an event start recorded against another clock |
| [`negative-function-contract-mismatch`](negative-function-contract-mismatch/) | reject an unavailable project-routine contract |
| [`negative-greatest-incomparable-sources`](negative-greatest-incomparable-sources/) | reject a last-known-alive date taken from a day number |
| [`negative-group-count-without-id`](negative-group-count-without-id/) | reject an unnamed baseline-count rule |
| [`negative-ingest-unparseable-field`](negative-ingest-unparseable-field/) | reject a dose recorded with its unit |
| [`negative-keys-internal-column`](negative-keys-internal-column/) | reject a site-scoped subject identity |
| [`negative-keys-missing-value`](negative-keys-missing-value/) | reject a record that no analysis visit identifies |
| [`negative-mapping-case-fold-collision`](negative-mapping-case-fold-collision/) | reject a smoking flag whose dictionary answers twice |
| [`negative-mapping-from-duplicate-key`](negative-mapping-from-duplicate-key/) | reject a reference range stated twice |
| [`negative-mapping-from-key-length-mismatch`](negative-mapping-from-key-length-mismatch/) | reject a reference range chosen by an unpaired key |
| [`negative-mapping-from-partial-key`](negative-mapping-from-partial-key/) | reject a reference range chosen without a sex |
| [`negative-mapping-from-unmapped-key`](negative-mapping-from-unmapped-key/) | reject a result with no reference range |
| [`negative-mapping-unmapped-value`](negative-mapping-unmapped-value/) | reject an unmapped response |
| [`negative-output-duplicate-subject`](negative-output-duplicate-subject/) | reject a repeated demographics record |
| [`negative-output-order-repeated-term`](negative-output-order-repeated-term/) | reject an order that places one value twice |
| [`negative-output-order-unknown-column`](negative-output-order-unknown-column/) | reject a submission order over a value the dataset does not carry |
| [`negative-query-slot-overflow`](negative-query-slot-overflow/) | reject an event belonging to more queries than it has places |
| [`negative-record-lookup-id-collision`](negative-record-lookup-id-collision/) | reject a first treatment named after its own source |
| [`negative-record-lookup-incomparable-range`](negative-record-lookup-incomparable-range/) | reject an epoch range with incomparable endpoints |
| [`negative-record-lookup-incomplete-key`](negative-record-lookup-incomplete-key/) | reject a reference limit chosen without a sex |
| [`negative-record-lookup-unmatched-key`](negative-record-lookup-unmatched-key/) | reject a result with no reference range |
| [`negative-record-lookup-unordered-choice`](negative-record-lookup-unordered-choice/) | reject a treatment and dose taken from an unchosen record |
| [`negative-record-lookup-unordered-keep`](negative-record-lookup-unordered-keep/) | reject a treatment ordered but not chosen |
| [`negative-record-lookup-unpaired-key`](negative-record-lookup-unpaired-key/) | reject a reference limit matched against nothing |
| [`negative-row-value-self-reference`](negative-row-value-self-reference/) | reject a weight carried forward from a carried-forward weight |
| [`negative-row-value-zero-offset`](negative-row-value-zero-offset/) | reject a previous weight that names no earlier visit |
| [`negative-source-duplicate-right-key`](negative-source-duplicate-right-key/) | reject duplicate subject enrichment |
| [`negative-source-output-self-reference`](negative-source-output-self-reference/) | reject a parameter that reads the dataset it is part of |
| [`negative-sum-non-numeric-source`](negative-sum-non-numeric-source/) | reject a severity burden totalled from severity words |
| [`negative-types-unknown-field`](negative-types-unknown-field/) | reject a total over a field the source does not have |
| [`negative-usubjid-exceeds-length`](negative-usubjid-exceeds-length/) | reject a subject identifier longer than the study permits |
| [`negative-variable-nested-expression`](negative-variable-nested-expression/) | reject an uppercased country chosen inside the same step |
| [`negative-verification-implausible-age`](negative-verification-implausible-age/) | reject an implausible age |
| [`odm-form-scoped-item-resolution`](odm-form-scoped-item-resolution/) | resolve items within their collection form |
| [`sdtm-ae-dictionary-coding`](sdtm-ae-dictionary-coding/) | code reported terms against a medical dictionary |
| [`sdtm-ae-effective-transaction`](sdtm-ae-effective-transaction/) | take the effective state of a record from a transaction log |
| [`sdtm-dm-basic`](sdtm-dm-basic/) | build one subject record from collected data |
| [`sdtm-dm-metadata-contract`](sdtm-dm-metadata-contract/) | declare the metadata a submission needs |
| [`sdtm-dm-reference-dates`](sdtm-dm-reference-dates/) | derive the reference dates from EX, DS, and AE |
| [`sdtm-ds-disposition-sequence`](sdtm-ds-disposition-sequence/) | number each subject's disposition records in date order |
| [`sdtm-ex-combination-regimen`](sdtm-ex-combination-regimen/) | represent a combination regimen |
| [`sdtm-fa-fever-occurrence`](sdtm-fa-fever-occurrence/) | fever occurrence |
| [`sdtm-lb-conditional-compartments`](sdtm-lb-conditional-compartments/) | tell an inapplicable compartment from an uncollected sample |
| [`sdtm-lb-ctcae-grading`](sdtm-lb-ctcae-grading/) | assign toxicity grades |
| [`sdtm-lb-findings`](sdtm-lb-findings/) | build one record per collected lab result |
| [`sdtm-lb-multiform`](sdtm-lb-multiform/) | consolidate four collection forms into one dataset |
| [`sdtm-lb-reference-range-indicator`](sdtm-lb-reference-range-indicator/) | apply external reference ranges |
| [`sdtm-relrec-many-to-many`](sdtm-relrec-many-to-many/) | record relationships between events and medications |
| [`sdtm-suppmh-parent-linkage`](sdtm-suppmh-parent-linkage/) | link qualifiers collected on their own form to a parent record |
| [`sdtm-suppmh-qualifiers`](sdtm-suppmh-qualifiers/) | reshape extra qualifiers into supplemental records |
| [`sdtm-vs-unit-standardization`](sdtm-vs-unit-standardization/) | standardize collected results into the study's units |
| [`sdtm-vs-visit-study-day`](sdtm-vs-visit-study-day/) | attach visit metadata and study day to a result |
