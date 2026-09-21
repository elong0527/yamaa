---
title: Benchmark
hide:
  - actions
---

# Benchmark

$total, generated from [`benchmarks/`]($source_url). Each one is a complete,
runnable specification with its input data and the exact output an
implementation must reproduce, and each page below shows that README, the
source fixtures, the expected artifacts, and the YAML specification together.

$groups

Rejection is half the contract. A portable specification language is defined
as much by what it refuses as by what it computes, and a refusal is only
portable if two implementations refuse the same thing in the same way, so
every anti-pattern pins the exact error alongside the specification that
provokes it.

The suite serves three audiences at once:

- **Users** read it as a cookbook: "how do I express a baseline flag" has an
  answer to copy.
- **Developers** run it as a conformance test: the expected files are
  byte-exact contracts.
- **AI agents** learn the common derivations from one half and the
  anti-patterns that bound them from the other.

## Start here

| # | Benchmark | What it establishes |
|---|---|---|
| 1 | [Basic Demographics](sdtm-dm-basic.html) | Reading collected items, handlers, and the declared keys as the grain |
| 2 | [Lab Findings](sdtm-lb-findings.html) | Row construction: one template per collected test, `row_number` for the sequence |
| 3 | [Lab BDS Build](adam-adlb-bds.html) | A full Basic Data Structure build: parameters as row templates, then baseline, change and sequence as columns |

Read a page top to bottom and the loop closes in five minutes: the Summary
gives the intent, the specification sidebar gives the shape and the grain, and
the expected artifact confirms the reading.

## Pick by question

| Question | Benchmark |
|---|---|
| How does one collected record become several analysis records? | [Lab BDS Build](adam-adlb-bds.html) -- two row templates sharing a filter |
| How do I carry ADSL values onto every event? | [Treatment-Emergent Flags](adam-adae-treatment-emergent.html) -- cross-dataset `source` on the applicable keys |
| How do I make several columns read **one** record? | [Death Carryforward](adam-adae-death.html) -- a named `intermediates` entry |
| How do I flag the baseline record and broadcast its value? | [Lab BDS Build](adam-adlb-bds.html) -- `baseline_flag`, then `aggregate` with `filter: "ABLFL = 'Y'"` and `expr: "ONLY(AVAL)"` |
| How do I total a subject's exposure records? | [Cumulative Dose](adam-adex-cumulative-dose.html) -- `aggregate: "SUM(EX.EXDOSE)"` |
| How do I impute a partial date and flag what was imputed? | [Partial Date Imputation](adam-adae-partial-dates.html) -- `date_impute` beside `date_precision` |
| How do I translate one value into three vocabularies? | [Demographics Standardization](adam-adsl-demographics.html) -- three `mapping` expressions over one source |
| Where does define.xml metadata go, and what gets enforced? | [Metadata Contract](sdtm-dm-metadata.html) -- `metadata` for documentation, `verifications` for enforcement |
| How do corporate, compound and study layers compose? | [Spec Inheritance](schema-inheritance.html) -- three levels, `expected/spec_resolved.yaml` records the outcome |
| When should a calculation leave the specification? | [BMI Function](adam-adsl-bmi-function.html) against [Compute BMI](adam-adsl-bmi-compute.html) -- versioned project function against closed expression |

## Reading the anti-patterns

An anti-pattern is a specification, its input, and an `expected/error.yaml`
stating the phase that rejects the run, a stable snake-case condition, the
spec paths implicated, and optional context. Every one of their READMEs ends
with a `How to fix` section that leads with the clinical decision and then
shows the smallest valid correction.

Reading them by family is faster than reading them alphabetically:

| Family | What they collectively pin |
|---|---|
| `negative-compute-*` | The closed `compute` grammar: no aggregates, no comparisons, no division by zero |
| `negative-date-impute-*`, `negative-datetime-*` | Every unusable temporal input has one defined outcome |
| `negative-record-lookup-*`, `negative-source-duplicate-key` | Matching must be complete, paired, unique, and ordered when it chooses |
| `negative-keys-*`, `negative-output-*` | Keys are an assertion, not documentation |
| `negative-adsl-*parent*`, `negative-inherited-output` | Inheritance failures: cycles, version mismatches, remote paths, who owns `output` |
| `negative-dose-expansion`, `negative-adlb-computed-param` | Where the language deliberately stops, and what to do upstream instead |

A specification that **cannot** express something is recorded as a design
finding in the issue tracker, so the last family doubles as the honest
inventory of what the language cannot yet do.

## All benchmarks

<nav class="benchmark-jump" aria-label="Jump to a group">$summary</nav>

$sections
