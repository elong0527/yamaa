---
title: YAML examples walkthrough
---

# A walkthrough of `yaml/examples/`

> **Audience:** anyone about to write, review, or implement a YAMAA
> specification.
>
> **Purpose:** explain what the example suite is, how to read one example,
> which example to open for which question, and what the negative examples
> cover.
>
> For the concepts themselves -- class, type, expression and the verb table --
> see [Schema concepts](schema-concepts.md).

---

## 1. What the suite is

`yaml/examples/` holds **149 directories**. Each one is a complete, runnable
specification with its input data and the exact output an implementation must
reproduce:

| Group | Count | What it is |
|---|---|---|
| `adam-*` | 65 | ADaM derivations |
| `sdtm-*` | 18 | SDTM derivations |
| `odm-*` | 1 | An ODM resolution behavior |
| `negative-*` | 65 | Specifications the design **must reject**, with the exact error |

Almost half the suite is negative. That ratio is the point: a portable
specification language is defined as much by what it refuses as by what it
computes, and a refusal is only portable if two implementations refuse the same
thing in the same way.

The suite serves three audiences at once:

1. **Implementers** run it as a conformance test -- the expected files are
   byte-exact contracts.
2. **Specification authors** read it as a cookbook -- "how do I express a
   baseline flag" has an answer you can copy.
3. **Designers** use it as the admission gate -- a construct enters the language
   only when an example needs it and a negative example pins its failure
   behavior.

---

## 2. Reading one example in five minutes

Take [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-basic), the suggested first read.

**Step 1 -- the README, for intent.** One record per subject; `SEX` is the
collected sex translated to `M`/`F`/`U`, and a sex that was never collected and
one the study does not recognise both become `U`; `AGE` is empty for a subject
whose age was never collected; a subject with no arm gets `Unassigned`.

A README describes **data, not the specification** -- what each output variable
means and what it holds when the inputs do not support it. So the README gives
you the clinical intent and `spec.yaml` gives you the construct, which is the
pair an Excel spec never separates.

**Step 2 -- the header of `spec.yaml`, for shape.**

```yaml
schema_version: "1.0"
domain: DM
datasets:
  ODM: input/odm.csv
base: ODM
keys: [STUDYID, USUBJID]

output:
  columns: [STUDYID, DOMAIN, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM]
```

One input, one driver, two keys, eight delivered columns.

**Step 3 -- the bottom of the file, for row count.**

```yaml
rows:
  - id: subject
    filter: "ODM.ItemOID = 'IT.DM.SEX'"
    derivations: {}
```

The row template derives nothing. It exists **only to establish the grain**:
one output row per subject SEX item record. This is the cleanest illustration
in the suite that row construction and column derivation are separate phases.

**Step 4 -- the columns that carry judgement.**

```yaml
  - name: SEX
    derivation:
      mapping:
        source: ODM.Value
        dict: {Male: M, Female: F}
        missing: U
        unmapped: U

  - name: AGE
    derivation:
      source:
        variable: ODM.IT.DM.AGE
        missing: null
```

Now the README's sentences have addresses. "Not collected and not recognised
both become U" is `missing: U` beside `unmapped: U`. "Empty when age was never
collected" is the structured `source` form with `missing: null` -- the concise
`source: ODM.IT.DM.AGE` form has no handler, so an absent item would be fatal.

**Step 5 -- `expected/dm.csv`.** Confirm your reading against the artifact. Every
sparse subject you predicted appears with the substituted value rather than
being dropped.

Five minutes, and you have the full loop: intent -> shape -> grain -> handlers ->
result.

---

## 3. The recommended reading path

The suite's own README names three examples, in this order:

| # | Example | What it establishes |
|---|---|---|
| 1 | [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-basic) | Direct mapping, handlers, a row template used only for grain |
| 2 | [`sdtm-lb-findings`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-findings) | Real row construction: one template per collected test, `row_number` for the sequence |
| 3 | [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds) | A full Basic Data Structure build: parameters as row templates, then baseline, change and sequence as columns |

After those three, pick by the question you have. Sections 4 and 5 are that
index.

---

## 4. Positive examples by construct

The suite's own index is organized by clinical outcome. What follows is the
complementary view -- **which example to open when you want to see a construct
in use, and which rule governs it.** Rule IDs are the normative pages in
[`yaml/rules/`](https://github.com/elong0527/yamaa/tree/main/yaml/rules).

Rule coverage across the 49 questions below:

| R001 | R002 | R003 | R004 | R005 | R006 | R007 | R008 | R009 | R010 | R011 | R012 | R013 | R014 | R015 | R016 | R017 | R018 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 8 | 3 | 2 | 0 | 1 | 0 | 15 | 3 | 1 | 0 | 1 | 1 | 7 | 0 | 8 | 5 | 2 | 3 |

### Row construction and value-level metadata

| Question | Rule | Example |
|---|---|---|
| How does one collected record become several analysis records? | R001 | [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds) -- `alt` and `alt_si` share a filter, so each ALT result produces two rows |
| How do I build one record per collected result rather than per scheduled test? | R001 | [`sdtm-lb-findings`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-findings) |
| How do I consolidate several collection forms into one domain? | R001, R002 | [`sdtm-lb-multiform`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-multiform) -- the same analyte on two forms is separated by specimen and location, not by test code |
| How do I add a derived parameter alongside collected ones? | R001 | [`adam-advs-body-mass-index`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-body-mass-index), [`adam-advs-body-surface-area`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-body-surface-area), [`adam-advs-mean-arterial-pressure`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-mean-arterial-pressure) |
| How do I reduce a driver group into one candidate row? | R001 | [`adam-advs-body-mass-index`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-body-mass-index) -- a grouped row template whose `filter` runs after the candidate is complete |
| How do I score a questionnaire subscale from its item records? | R001, R013 | [`adam-adqs-subscale-score`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adqs-subscale-score) |
| How do I reshape extra qualifiers into supplemental records? | R001 | [`sdtm-suppmh-qualifiers`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-suppmh-qualifiers), [`sdtm-suppmh-parent-linkage`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-suppmh-parent-linkage) |

### Cross-dataset enrichment

| Question | Rule | Example |
|---|---|---|
| How do I carry ADSL values onto every event without writing a merge? | R003 | [`adam-adae-treatment-emergent`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-treatment-emergent) -- `source: ADSL.TRTSDT` joins on the applicable keys |
| How do I make several columns read **one** record? | R015 | [`adam-adae-death-outcome`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-death-outcome) -- a `record_lookups` entry named `DEATHEV`, read by both the cause and the date |
| How do I look up a value in a table keyed on something other than my output keys? | R015 | [`sdtm-ae-dictionary-coding`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ae-dictionary-coding) (MedDRA) and [`sdtm-suppmh-parent-linkage`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-suppmh-parent-linkage) use `mapping_from` |
| How do I select a record by a range rather than by equality? | R015 | [`adam-advs-analysis-window-table`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-analysis-window-table) -- a record lookup with `between: {value: ADY, lower: AWLO, upper: AWHI}`, so the window boundaries stay in the study's table |
| How do I choose one of several matching records deterministically? | R003, R015 | [`adam-adsl-rescue-medication`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-rescue-medication) and [`adam-adsl-treatment-selection`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-treatment-selection) use `multiple_matches`; [`sdtm-lb-reference-range-indicator`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-reference-range-indicator) uses an ordered record lookup |
| How do I take the effective state from a transaction log? | R015 | [`sdtm-ae-effective-transaction`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ae-effective-transaction) |

### Windows, baselines and ordering

| Question | Rule | Example |
|---|---|---|
| How do I flag the baseline record and broadcast its value? | R007 | [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds) -- `baseline_flag` then `baseline_value`; also [`adam-adlb-shift-and-criteria`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-shift-and-criteria) |
| How do I carry a result through later planned gaps? | R007, R015 | [`adam-advs-once-measured-carry-forward`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-once-measured-carry-forward) -- a record lookup plus `previous_non_missing`; [`adam-advs-prior-character-result`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-prior-character-result) pins missing groups, missing order values, and ties |
| How do I carry one selected characteristic onto every record? | R007, R015 | [`adam-advs-once-measured-carry-forward`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-once-measured-carry-forward) -- `baseline_value` broadcasts one selected height, rather than following a changing predecessor |
| How do I number records once they all exist? | R007 | [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-bds) (`ASEQ`), [`sdtm-ds-disposition-sequence`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-ds-disposition-sequence) |
| How do I rank with ties? | R007 | [`adam-adae-severity-rank`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-severity-rank) |
| How do I read the neighbouring row's value? | R007 | [`adam-adex-dose-reduction-flag`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-dose-reduction-flag) and [`adam-adrs-confirmed-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adrs-confirmed-response) use `row_value`; [`negative-adrs-partial-response-after-complete-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adrs-partial-response-after-complete-response) shows the assertion pattern built on it |
| How do I flag the first occurrence at several levels? | R007 | [`adam-adae-occurrence-flags`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-occurrence-flags) |
| How do I pick the record closest to a window's target day? | R007, R015 | [`adam-adlb-closest-visit`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-closest-visit), [`adam-advs-analysis-window-table`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-analysis-window-table) |

### Aggregation

| Question | Rule | Example |
|---|---|---|
| How do I total a subject's exposure records? | R013 | [`adam-adex-cumulative-dose`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-cumulative-dose) -- `aggregate: "SUM(EX.EXDOSE)"` with no `group_by`, reducing by the applicable keys |
| How do I broadcast a group mean back onto each row? | R013 | [`adam-adlb-mean`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-mean) -- an unqualified aggregate with an explicit `group_by` |
| How do I take the earliest of many records as a reference date? | R013 | [`adam-adsl-dependency-order`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-dependency-order) -- `MIN(EX.EXSTDTC)` behind a `filter` |
| How do I sum measurements per assessment? | R013 | [`adam-adtr-sum-of-target-diameters`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtr-sum-of-target-diameters) |
| How do I derive a nadir? | R013 | [`adam-adtr-current-nadir`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtr-current-nadir) |

### Dates

| Question | Rule | Example |
|---|---|---|
| How do I impute a partial date and flag what was imputed? | R016, R008 | [`adam-adae-partial-dates`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-partial-dates) -- `date_impute` beside `date_precision` reading the same source |
| How do I compute an age in whole years? | R016 | [`adam-adsl-analysis-age`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-analysis-age) -- `date_diff` with `unit: year` and explicit `bounds` |
| How do I compute a study day? | R016 | [`sdtm-vs-visit-study-day`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-vs-visit-study-day), [`adam-adsl-randomization-timing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-randomization-timing) |
| How do I derive a time-to-event endpoint? | R016 | [`adam-adtte-overall-survival`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-overall-survival), [`adam-adtte-progression-free-survival`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-progression-free-survival), [`adam-adtte-duration-of-response`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adtte-duration-of-response) |
| How do I scope treatments and dates to a period across a washout? | R013, R016 | [`adam-adsl-crossover-periods`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-crossover-periods) |

### Strings, codelists and classification

| Question | Rule | Example |
|---|---|---|
| How do I parse an identifier and fall back to a collected value? | R007, R012 | [`adam-adsl-identifier-parsing`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-identifier-parsing) -- `str_extract`, then `coalesce`, then `str_template` |
| How do I translate one collected value into three vocabularies? | R007 | [`adam-adsl-mapping`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-mapping) -- three `mapping` expressions over the same source |
| How do I band a numeric value? | R007 | [`adam-adsl-mapping`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-mapping) (`AGEGR1` via `cut`), [`adam-adsl-dependency-order`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-dependency-order) |
| How do I build USUBJID from parts? | R007 | [`sdtm-dm-metadata-contract`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-metadata-contract) -- `str_concat` mixing sources and literals |
| How do I clean text and reject a malformed identifier? | R007, R008 | [`adam-adae-string-handlers`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adae-string-handlers) |
| How do I normalize a country and group it into a region? | R007 | [`adam-adsl-geography-normalization`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-geography-normalization) |
| How do I classify a result, its shift, and a criterion flag? | R007 | [`adam-adlb-shift-and-criteria`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-shift-and-criteria) |

### Contracts, metadata and checks

| Question | Rule | Example |
|---|---|---|
| Where does define.xml metadata go, and what actually gets enforced? | R005, R009 | [`sdtm-dm-metadata-contract`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-metadata-contract) -- `metadata` for documentation, `verifications` for enforcement |
| How do I chain population flags in dependency order? | R001 | [`adam-adsl-dependency-order`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-dependency-order) -- each flag reads the previous one, and `RANDFL` stays internal |
| What happens to a non-finite number? | R011 | [`adam-adsl-non-finite-values`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-non-finite-values) -- nine derived values from YAML, source fields and a project function, all normalized to missing |
| How do I distinguish an uncollected value from an inapplicable one? | R008 | [`adam-adex-uncollected-exposure`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adex-uncollected-exposure), [`sdtm-lb-conditional-compartments`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-conditional-compartments) |

### Inheritance

| Question | Rule | Example |
|---|---|---|
| How do corporate, compound and study layers compose? | R017 | [`adam-adlb-standardized-result`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adlb-standardized-result) -- four layers resolve `organization -> compound -> study -> spec`, and `expected/resolved.yaml` records the outcome: shorthand expanded to canonical form, member fields merged while root fields are replaced whole, unreachable declarations pruned, and layer-relative paths rebased to the entry file |
| How does inheritance fail? | R017 | The four `negative-adsl-*parent*` examples plus [`negative-adsl-inherited-output`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-inherited-output) -- a cycle, a version mismatch, a remote path, an invalid clear, and an `output` an entry file may not inherit |

### Project functions

| Question | Rule | Example |
|---|---|---|
| When should a calculation leave the specification? | R018 | [`adam-adsl-bmi-function`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-function) versus [`adam-adsl-bmi-compute`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-adsl-bmi-compute) -- same input, same artifact; the difference is a closed numeric expression any implementation can evaluate versus a versioned project binding with a digest and a conformance vector |
| What does a calculation with no portable closed form look like? | R018 | [`adam-advs-growth-percentile`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/adam-advs-growth-percentile) |
| What happens when the environment supplies the wrong contract? | R018 | [`negative-function-contract-mismatch`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-function-contract-mismatch) -- a spec requesting `2.0.0` against an environment supplying `1.0.0` |

### ODM specifics

| Question | Rule | Example |
|---|---|---|
| How is an item resolved inside its collection form? | R002 | [`odm-form-scoped-item-resolution`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/odm-form-scoped-item-resolution) -- identical item identifiers in two forms are different values and must not be collapsed |
| How does a contextual item reference work in practice? | R002 | [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-dm-basic), [`sdtm-lb-findings`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/sdtm-lb-findings) |

---

## 5. Negative examples

A negative example is a specification, its input, and an `expected/error.yaml`
stating the `phase` that rejects the run, a stable snake-case `condition`, the
`spec_paths` implicated, and optional `context`. It is a partial assertion: an
implementation may add context and word its message however it likes, but the
stated fields must match. Every negative README ends with a `## How to fix`
section that leads with the clinical decision and then shows the smallest valid
correction -- never a weakened check.

Reading them by family is faster than reading them alphabetically:

| Family | Examples | What they collectively pin |
|---|---|---|
| Closed `compute` grammar | `negative-compute-*` (7) | Aggregate functions, comparison operators, qualified identifiers, division by zero, integer overflow, `LN(0)`, `SQRT(-1)` |
| Predicate grammar | [`negative-adae-review-condition-arithmetic`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adae-review-condition-arithmetic), `-review-text-date`, `-review-unknown-date` | An operand is a name or a literal; types must be comparable; names must resolve |
| Type conversion | `negative-conversion-*`, [`negative-ingest-unparseable-field`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-ingest-unparseable-field), [`negative-column-type-unknown`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-column-type-unknown) | Conversion fails loudly rather than substituting a value |
| Partial dates | `negative-date-impute-*` (4), [`negative-date-precision-invalid-source`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-date-precision-invalid-source), [`negative-datetime-zone-offset`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-datetime-zone-offset) | Every unusable temporal input has one defined outcome |
| Lookups and joins | `negative-record-lookup-*` (7), `negative-mapping-from-*` (4), [`negative-source-duplicate-right-key`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-source-duplicate-right-key) | Matching must be complete, paired, unique, and ordered when it chooses |
| Output identity | `negative-keys-*`, [`negative-output-duplicate-subject`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-output-duplicate-subject), [`negative-usubjid-exceeds-length`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-usubjid-exceeds-length) | Keys are an assertion, not documentation |
| Inheritance | `negative-adsl-*parent*` (4), [`negative-adsl-inherited-output`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adsl-inherited-output) | Cycles, version mismatches, remote paths, invalid clears, and who owns `output` |
| Expressiveness limits | [`negative-adex-single-dose-expansion`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adex-single-dose-expansion), [`negative-adlb-computed-parameter`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-adlb-computed-parameter), [`negative-query-slot-overflow`](https://github.com/elong0527/yamaa/tree/main/yaml/examples/negative-query-slot-overflow) | Where the language deliberately stops, and what to do upstream instead |

An example that **cannot express something** is recorded as a design finding in
the issue tracker, so the last family is also the honest inventory of what the
language cannot yet do.
