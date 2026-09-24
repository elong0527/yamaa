# Uncollected codes stay missing; outside-codelist codes are labelled

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-text-mapping-unmapped.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin how a dictionary lookup tells a missing code from a collected
code outside the dictionary.

**Input:** `spec.yaml` decodes coded sex from `DM` twice: once with a
separate named result for codes outside the dictionary, once with one named
result for both events. `DM` carries listed codes, one missing code, and one
code outside the dictionary.

**Note:** a missing code means "not collected"; a collected code outside the
dictionary means "outside the codelist" and is usually a data-quality
finding. The `SEXC` column keeps the missing code missing and reads the
outside code as `Outside codelist`. The `SEXC_SINGLE` column reads both
events as `Unknown`, the single-result behavior the lookup had before.

**Standard:** CDISC | **Domain:** ADSL
