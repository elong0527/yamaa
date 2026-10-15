# Clinical Site Data at Site Level

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/bimo-clinsite-site-level.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per clinical site for a Bioresearch Monitoring
(BIMO) submission, with the site identifier (`SITEID`), the assigned
treatment arm (`ARM`), the number of treated subjects (`SAFPOP`), and the
number of enrolled subjects (`ENRLPOP`).

**Input:** one row per subject carrying the study identifier, the unique
subject identifier, the site key, the subject's treatment arm, and the
treated flag.

**Variables:**

- `SITEID` is the site key exactly as collected, leading zeros kept, so a
  site key of `007` and a site key of `7` are different sites.
- `ARM` is the alphabetically first arm among the site's treated subjects;
  it has no value when the site has no treated subjects.
- `SAFPOP` is the number of treated subjects at the site; it is zero for
  a site whose subjects were all untreated.
- `ENRLPOP` is the number of enrolled subjects at the site, treated or
  not.

**Note:** a site appears in the dataset exactly when at least one subject
row names it. A site with only untreated subjects still gets a record, with
no arm value and a treated count of zero, while a site named by no subject
row gets no record at all.

**Standard:** BIMO | **Domain:** CLINSITE
