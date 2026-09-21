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

## All benchmarks

<nav class="benchmark-jump" aria-label="Jump to a group">$summary</nav>

$sections
