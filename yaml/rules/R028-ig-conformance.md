---
id: R028
title: IG Domain-Model Conformance
status: normative
applies_to: [define.datasets, define.standards, root.domain, root.output,
  column.submission]
---

# IG domain-model conformance

## Intent

Check that a dataset's declared columns conform to the implementation guide
its study document binds it to: every required variable present, every
expected variable accounted for, no misspelled variables, and declared core
designations that agree with the guide. The language is standard-agnostic by
design, so without this check a specification can silently omit a variable
its IG requires and nothing catches it.

## Boundaries

This rule owns the conformance check: when it runs, what it compares, the
four conditions it reports, and their severity. R026 owns the study document
and the composition this check runs inside, including the standard binding
R026-7 declares. R024 owns the standard families, the `core` designation,
and the rule that a specification never names the standard. R025 owns
codelists and terminology. This rule reads no data: it compares the
specification's text against the IG's domain model.

## When the check runs

**R028-1.** The check runs at study-document composition (R026-10), where
the resolved specification, the bound standard, and its version meet. A
specification validated without a study document is IG-agnostic, and the
check is skipped. There is no standalone mode: R024-5 forbids a
specification from naming the standard, so a lone specification carries no
binding to check against.

**R028-2.** The binding is the study document's: each `datasets` entry's
`standard` resolves to a declared standard of type `IG` carrying a published
`name` and `version` exactly as published (R026-7). The domain is the
specification root's `domain`.

## Templates

**R028-3.** The comparison is against a template keyed by (family, standard
name, version, domain), for example `sdtm / SDTMIG / 3.4 / DM`. The family
is read from R024-5's table. This rule checks the `sdtm` and `send`
families; a dataset bound to any other family is not checked here.

**R028-4.** Templates are versioned data, one file per (standard name,
version, domain), each listing the domain's variables with their `core`
designation and label. A new IG release adds template files. No rule or
schema change accompanies a new release.

**R028-5.** A bound (name, version, domain) with no template yields
`ig_no_template`: an explicit signal that the check could not run, never a
silent skip. A SUPPQUAL dataset, whose variables are sponsor-defined by
design, has no template and reports this signal.

## The checks

**R028-6.** Every template variable designated `Req` must appear among the
dataset's `output.columns`. A missing required variable fails validation
with `ig_missing_required`, naming the variable and the template.

**R028-7.** Every template variable designated `Exp` should appear among
`output.columns`. A missing expected variable is a reviewer signal
`ig_missing_expected`, never a failure: omitting an expected variable is
legitimately justifiable, and the justification belongs in the reviewer's
guide.

**R028-8.** An output column naming no template variable is a reviewer
signal `ig_unknown_variable`. When exactly one template variable is within
edit distance 2 of the column name, the signal names it as a possible
misspelling. The signal never fails: the IG permits nonstandard variables
with define documentation.

**R028-9.** A column declaring `submission.core` that differs from the
template's core designation for the same variable is a reviewer signal
`ig_core_mismatch`, naming both designations. A column declaring no `core`
is not compared.

**R028-10.** `Perm` variables are never flagged for omission.

## Severity

**R028-11.** `ig_missing_required` fails validation. Every other condition
in this rule is a signal carried in the conformance report. A missing
required variable is a specification defect, consistent with R024's
hard failure on `Req` violations; the remaining conditions record reviewer
judgment, not defects.

## Static comparison

**R028-12.** The check compares specification text against template text.
It reads no source data and no artifact. Whether a declared variable is
populated at runtime is outside this rule.

## Rationale

The binding already exists in the study document, so this rule adds no
specification syntax: the check is a pure function of composition inputs.
Templates are data rather than rule text because IG releases version
independently of this language; generating them from CDISC-published domain
metadata keeps them honest, and hand-authored seeds are marked as such in
their headers. Severity splits on `Req` because `Req` means required: the
IG leaves no room for judgment there, while `Exp`, unknown variables, and
core disagreements all admit legitimate explanations a reviewer weighs.

## Errors

| Condition | Requirement | Severity | Meaning |
|---|---|---|---|
| `ig_missing_required` | R028-6 | error | A template `Req` variable is absent from `output.columns`. |
| `ig_missing_expected` | R028-7 | signal | A template `Exp` variable is absent from `output.columns`. |
| `ig_unknown_variable` | R028-8 | signal | An output column names no template variable; a closest match within edit distance 2 is named when unique. |
| `ig_core_mismatch` | R028-9 | signal | The column's declared `core` differs from the template's. |
| `ig_no_template` | R028-5 | signal | No template exists for the bound (name, version, domain); the check did not run. |
