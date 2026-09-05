---
title: YAMAA documentation
---

YAMAA is a language-neutral YAML specification for reproducible clinical trial
data pipelines that transform ODM data into SDTM and ADaM datasets following
CDISC standards.

These pages are written for statistical programmers who already write SDTM and
ADaM specifications in Excel. Each one has a different job.

## Start here

**[Why YAMAA looks the way it does](why-yamaa.md)**
An Excel specification has three layers and writes down only two. This page is
about the missing one -- what "semantics" means in practice, why two programmers
reading the same spec write different programs, and what YAMAA deliberately
refuses to do about it. Start here if you want the argument before the syntax.

**[Translating an Excel specification](excel-to-yamaa.md)**
The same specification written twice, then every column of a Dataset sheet and
a Variable sheet mapped to its YAMAA field, then nine specifications worked
through in full -- direct mapping, computed values, predecessor joins, a BDS
build, aggregation, partial dates, define.xml metadata, dictionary coding, and
template inheritance. Start here if you have specifications to convert.

**[The schema: class, type, expression, registry](schema-concepts.md)**
The four words you need to read `yaml/schema*.yaml`, and the complete table of
derivation verbs with what each one is for. Start here if you are writing or
reviewing a specification and need the language itself.

**[A walkthrough of the examples](yaml-examples-walkthrough.md)**
How the 149 example directories are put together, which example answers which
question, and how the negative examples encode failure behavior. Start here if
you are implementing YAMAA, or looking for a worked precedent.

## Teaching from these pages

With one hour, and an audience of statistical programmers:

1. **The three-layer table in [Why YAMAA](why-yamaa.md), and 1.1 under it** --
   establish that an Excel spec is missing its semantics layer, and that double
   programming is what teams currently use to reconstruct it.
2. **[Example 1](excel-to-yamaa.md#example-1-direct-mapping-a-codelist-and-numeric-banding)**
   (`mapping` and `cut`) -- closest to Excel, lowest barrier; emphasise
   splitting `missing` from `unmapped`.
3. **[Example 3](excel-to-yamaa.md#example-3-predecessor-and-the-automatic-left-join)**
   (the automatic join) -- how `keys` replaces "merge by".
4. **[Example 4](excel-to-yamaa.md#example-4-vlm-and-bds-in-one-spec)**
   (value-level metadata and a BDS build) -- the two-phase model and row
   templates; the densest segment of the session.
5. **What is deliberately absent** ([Why YAMAA](why-yamaa.md), section 3)
   -- leave room for questions, especially about `ROUND`.

If the audience owns corporate standards, swap the emphasis to
**[example 9](excel-to-yamaa.md#example-9-organization-compound-and-study-layers)**
(inheritance) and
**project functions** ([Schema concepts](schema-concepts.md), section 2.8):
those answer how a standard is versioned, distributed and validated, which is
where an Excel template plus a macro-library SOP is weakest.

If the audience will write specifications, send them through the
[examples walkthrough](yaml-examples-walkthrough.md).

## Source

- [`yaml/rules/`](https://github.com/elong0527/yamaa/tree/main/yaml/rules) --
  the 20 normative rules, one topic each. These pages cite them as R001-R020;
  the index there says what each one owns.
- [`yaml/`](https://github.com/elong0527/yamaa/tree/main/yaml) -- the schema
  bundle
- [`yaml/examples/`](https://github.com/elong0527/yamaa/tree/main/yaml/examples)
  -- 163 runnable examples with exact expected output
