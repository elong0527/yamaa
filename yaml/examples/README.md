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
passes when its declared error occurs. Those findings are collected in
[`plan.md`](plan.md), which also tracks the schema work they justify.

Expected-failure examples carry `expected/error.yaml`. When rejection happens
after the dataset is completed, an expected CSV records the rows presented to
the failing check. When a missing capability prevents execution, an expected
CSV records the intended artifact once that capability exists.

| Example | Derives |
|---|---|
| [`adam-adae-occurrence-flags`](adam-adae-occurrence-flags/) | flag the first occurrence at three levels |
| [`adam-adae-partial-dates`](adam-adae-partial-dates/) | impute partial dates |
| [`adam-adae-post-dose-onset`](adam-adae-post-dose-onset/) | classify an event by the moment it started |
| [`adam-adae-severity-override`](adam-adae-severity-override/) | apply an approved severity correction |
| [`adam-adae-string-handlers`](adam-adae-string-handlers/) | clean text and handle invalid IDs |
| [`adam-adae-treatment-emergent`](adam-adae-treatment-emergent/) | classify an event as treatment-emergent |
| [`adam-adae-worst-severity`](adam-adae-worst-severity/) | flag the worst-severity event per preferred term |
| [`adam-adex-cumulative-dose`](adam-adex-cumulative-dose/) | summarize cumulative exposure |
| [`adam-adex-uncollected-exposure`](adam-adex-uncollected-exposure/) | tell an uncollected dose from an absent administration |
| [`adam-adlb-bds`](adam-adlb-bds/) | build a BDS dataset with baseline and change |
| [`adam-adlb-closest-visit`](adam-adlb-closest-visit/) | select the record closest to a window's target day |
| [`adam-adrs-best-overall-response`](adam-adrs-best-overall-response/) | select the best overall response |
| [`adam-adrs-composite-response`](adam-adrs-composite-response/) | combine efficacy, safety, and discontinuation into one response |
| [`adam-adrs-confirmed-response`](adam-adrs-confirmed-response/) | confirm an objective response |
| [`adam-adrs-overall-response-records`](adam-adrs-overall-response-records/) | prepare the overall response records an endpoint reads |
| [`adam-adsl-bmi-compute`](adam-adsl-bmi-compute/) | compute BMI from height and weight |
| [`adam-adsl-bmi-function`](adam-adsl-bmi-function/) | compute BMI by calling a routine the project supplies |
| [`adam-adsl-crossover-periods`](adam-adsl-crossover-periods/) | derive period-scoped treatments and dates across a washout |
| [`adam-adsl-dependency-order`](adam-adsl-dependency-order/) | derive a chain of population flags |
| [`adam-adsl-disposition`](adam-adsl-disposition/) | select the final subject disposition from DS |
| [`adam-adsl-geography-normalization`](adam-adsl-geography-normalization/) | normalize collected country and group it into a region |
| [`adam-adsl-identifier-parsing`](adam-adsl-identifier-parsing/) | parse the site from USUBJID with a collected fallback |
| [`adam-adsl-mapping`](adam-adsl-mapping/) | translate collected values into a standard vocabulary |
| [`adam-adsl-new-anticancer-therapy-date`](adam-adsl-new-anticancer-therapy-date/) | date the subject started new anti-cancer therapy |
| [`adam-adsl-population-flags`](adam-adsl-population-flags/) | derive the safety and intent-to-treat flags |
| [`adam-adsl-treatment-selection`](adam-adsl-treatment-selection/) | select actual treatment and its duration from EX |
| [`adam-adtr-sum-of-target-diameters`](adam-adtr-sum-of-target-diameters/) | sum the target lesion diameters at each assessment |
| [`adam-adtte-duration-of-response`](adam-adtte-duration-of-response/) | derive the duration of a response |
| [`adam-adtte-progression-free-survival`](adam-adtte-progression-free-survival/) | derive progression-free survival |
| [`adam-advs-analysis-visit`](adam-advs-analysis-visit/) | assign records to analysis windows |
| [`adam-advs-once-measured-carry-forward`](adam-advs-once-measured-carry-forward/) | carry forward a once-measured characteristic |
| [`negative-adam-adsl-stratification-reconciliation`](negative-adam-adsl-stratification-reconciliation/) | reconcile randomization strata |
| [`negative-adex-relative-dose-intensity`](negative-adex-relative-dose-intensity/) | reject a dose intensity measured against a per-record plan |
| [`negative-adrs-partial-response-after-complete-response`](negative-adrs-partial-response-after-complete-response/) | reject a partial response recorded after a complete response |
| [`negative-adrs-response-before-progression`](negative-adrs-response-before-progression/) | reject a best response restricted to pre-progression assessments |
| [`negative-adsl-subject-reference`](negative-adsl-subject-reference/) | reject a malformed subject reference |
| [`negative-column-type-unknown`](negative-column-type-unknown/) | reject a column with no stated kind of value |
| [`negative-datetime-zone-offset`](negative-datetime-zone-offset/) | reject an event start recorded against another clock |
| [`negative-mapping-unmapped-value`](negative-mapping-unmapped-value/) | reject an unmapped response |
| [`negative-row-value-self-reference`](negative-row-value-self-reference/) | reject a weight carried forward from a carried-forward weight |
| [`negative-source-duplicate-right-key`](negative-source-duplicate-right-key/) | reject duplicate subject enrichment |
| [`odm-form-scoped-item-resolution`](odm-form-scoped-item-resolution/) | resolve items within their collection form |
| [`sdtm-ae-dictionary-coding`](sdtm-ae-dictionary-coding/) | code reported terms against a medical dictionary |
| [`sdtm-ae-effective-transaction`](sdtm-ae-effective-transaction/) | take the effective state of a record from a transaction log |
| [`sdtm-dm-basic`](sdtm-dm-basic/) | build one subject record from collected data |
| [`sdtm-dm-metadata-contract`](sdtm-dm-metadata-contract/) | declare the metadata a submission needs |
| [`sdtm-dm-reference-dates`](sdtm-dm-reference-dates/) | derive the reference dates from EX, DS, and AE |
| [`sdtm-ex-combination-regimen`](sdtm-ex-combination-regimen/) | represent a combination regimen |
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
