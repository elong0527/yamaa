# Reading the examples

## What the suite is

`benchmark/` holds **213 directories**. Each one is a complete, runnable
specification with its input data and the exact output an implementation must
reproduce:

| Group | Count | What it is |
|---|---|---|
| `adam-*` | 76 | ADaM derivations |
| `sdtm-*` | 22 | SDTM derivations |
| `negative-*` | 109 | Specifications the design **must reject**, with the exact error |
| `odm-*` | 1 | An ODM resolution behavior |
| `schema-*`, `spec-*` | 5 | Schema and inheritance behaviors |

Half the suite is negative. That ratio is the point: a portable specification
language is defined as much by what it refuses as by what it computes, and a
refusal is only portable if two implementations refuse the same thing in the
same way.

The suite serves three audiences at once:

1. **Implementers** run it as a conformance test -- the expected files are
   byte-exact contracts.
2. **Specification authors** read it as a cookbook -- "how do I express a
   baseline flag" has an answer you can copy.
3. **Designers** use it as the admission gate -- a construct enters the
   language only when an example needs it and a negative example pins its
   failure behavior.

## Reading one example in five minutes

Take [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-dm-basic), the suggested first read.

**Step 1 -- the README, for intent.** One record per subject; `SEX` is the
collected sex translated to `M`/`F`/`U`, and a sex that was never collected and
one the study does not recognise both become `U`; `AGE` is empty for a subject
whose age was never collected; a subject with no arm gets `Unassigned`. A
README describes **data, not the specification** -- what each output variable
means and what it holds when the inputs do not support it.

**Step 2 -- the header of `spec.yaml`, for shape.**

```yaml
schema_version: "1.0"
domain: DM
keys: [STUDYID, USUBJID]

input:
  ODM: input/odm.csv

output:
  path: dm.csv
  columns: [STUDYID, DOMAIN, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM]
```

One input, one driver, two keys, eight delivered columns, one produced file.

**Step 3 -- what is not in the file, for row count.** There is no `rows:`
block, and `keys: [STUDYID, USUBJID]` is the whole answer. The distinct
`STUDYID`/`USUBJID` combinations the extract carries are the output rows, in
first-appearance order. Row construction and column derivation are separate
phases -- the grain names no column value, and no filter decides how many
records come out.

**Step 4 -- the columns that carry judgement.**

```yaml
  - name: SEX
    derivation:
      mapping:
        source:
          filter: ODM.ItemOID = 'IT.DM.SEX'
          variable: ODM.Value
        dict: {Male: M, Female: F}
        missing: U
        unmapped: U
```

Now the README's sentences have addresses. "Not collected and not recognised
both become U" is `missing: U` beside `unmapped: U`.

**Step 5 -- `expected/dm.csv`.** Confirm your reading against the artifact.

Five minutes, and you have the full loop: intent -> shape -> grain ->
handlers -> result.

## The recommended reading path

| # | Example | What it establishes |
|---|---|---|
| 1 | [`sdtm-dm-basic`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-dm-basic) | Reading collected items, handlers, and the declared keys as the grain |
| 2 | [`sdtm-lb-findings`](https://github.com/elong0527/yamaa/tree/main/benchmark/sdtm-lb-findings) | Real row construction: one template per collected test, `row_number` for the sequence |
| 3 | [`adam-adlb-bds`](https://github.com/elong0527/yamaa/tree/main/benchmark/adam-adlb-bds) | A full Basic Data Structure build: parameters as row templates, then baseline, change and sequence as columns |

After those three, pick by the question you have:

| Question | Example |
|---|---|
| How do I build one record per collected result? | `sdtm-lb-findings` |
| How does one collected record become several analysis records? | `adam-adlb-bds` -- two row templates sharing a filter |
| How do I carry ADSL values onto every event? | `adam-adae-treatment-emergent` -- cross-dataset `source` on the applicable keys |
| How do I make several columns read **one** record? | `adam-adae-death-outcome` -- a named `intermediates` entry |
| How do I flag the baseline record and broadcast its value? | `adam-adlb-bds` -- `baseline_flag` then `baseline_value` |
| How do I total a subject's exposure records? | `adam-adex-cumulative-dose` -- `aggregate: "SUM(EX.EXDOSE)"` |
| How do I impute a partial date and flag what was imputed? | `adam-adae-partial-dates` -- `date_impute` beside `date_precision` |
| How do I translate one value into three vocabularies? | `adam-adsl-mapping` -- three `mapping` expressions over one source |
| Where does define.xml metadata go, and what gets enforced? | `sdtm-dm-metadata-contract` -- `metadata` for documentation, `verifications` for enforcement |
| How do corporate, compound and study layers compose? | `spec-inheritance` -- three levels, `expected/spec_resolved.yaml` records the outcome |
| When should a calculation leave the specification? | `adam-adsl-bmi-function` vs `adam-adsl-bmi-compute` -- closed expression vs versioned project function |

The full construct-by-construct index lives in the suite's own
[benchmark README](https://github.com/elong0527/yamaa/tree/main/benchmark#readme);
every example is browsable with its input and expected output at
[Benchmark](../benchmark/index.md).

## Negative examples

A negative example is a specification, its input, and an `expected/error.yaml`
stating the phase that rejects the run, a stable snake-case condition, the
spec paths implicated, and optional context. Every negative README ends with a
`## How to fix` section that leads with the clinical decision and then shows
the smallest valid correction.

Reading them by family is faster than reading them alphabetically:

| Family | What they collectively pin |
|---|---|
| `negative-compute-*` | The closed `compute` grammar: no aggregates, no comparisons, no division by zero |
| `negative-date-impute-*`, `negative-datetime-*` | Every unusable temporal input has one defined outcome |
| `negative-record-lookup-*`, `negative-source-duplicate-right-key` | Matching must be complete, paired, unique, and ordered when it chooses |
| `negative-keys-*`, `negative-output-*` | Keys are an assertion, not documentation |
| `negative-adsl-*parent*`, `negative-adsl-inherited-output` | Inheritance failures: cycles, version mismatches, remote paths, who owns `output` |
| `negative-adex-single-dose-expansion`, `negative-adlb-computed-parameter` | Where the language deliberately stops, and what to do upstream instead |

An example that **cannot express something** is recorded as a design finding in
the issue tracker, so the last family is also the honest inventory of what the
language cannot yet do.
