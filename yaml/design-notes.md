# Design notes

These notes are non-normative. The [rule index](rules/README.md) and schema
entries define current behavior. This page records why the design took its
present shape and how documentation ownership has changed.

## Named inputs and limited nesting

Named inputs expose dependencies and keep individual operations reviewable.
Intermediate columns make multi-step calculations explicit without requiring
them in the published artifact. R007 owns the nesting boundary, the schema
entries own operation parameters, and R005 owns output membership.

## Closed expression languages

A keyword per arithmetic operator made one formula require several columns.
Likewise, separate reducer expressions could not compose arithmetic over
reductions. Closed numeric and aggregate languages allow readable formulas
while keeping dependencies discoverable. Predicates and string templates
have similarly limited purposes. Their executable grammars live in
[grammar/](grammar/README.md); R004, R010, R012, and R013 own their semantics.

## Explicit grain and record selection

Separating row construction from column derivation makes changes to output
grain visible. A named intermediate lets several columns share one selected
record. Materializing intermediate grains makes multi-specification workflows
reviewable without inferring pipeline order from filenames. R001, R003, and
R013 own these decisions.

## Evaluation and presentation

Intermediate columns and artifact ordering let a calculation's dependency
order differ from its presentation. Keeping presentation at the end prevents
display precision or output order from changing calculation and verification
results. R005 and R020 own these contracts.

## Project functions

A logical function contract separates a portable specification from the
project implementation. A single pinned runtime and shared conformance
vectors make that extension reproducible. R018 owns the extension boundary.

## Ownership migration

The rule reorganization groups the existing files into eight reading blocks.
It changes neither the YAML surface nor execution behavior. Files and existing
requirement IDs are retained. The index includes the R003 intermediate
contract and R027 Parquet profile; R015 remains retired. Requirements moved
between owners leave short numbered references, so existing examples and
error reports still resolve. New references should use the canonical owner
below.

| Existing reference | Canonical owner | Subject |
| --- | --- | --- |
| R007-19 | R011-34 | Input compatibility and no implicit input conversion |
| R007-31 | R011-35 | Runtime-type comparability |
| R007-8 | R013-3 | Permitted aggregate contexts; R007 retains aggregate registration |
| R007-9, R007-10 | R013-3, R013-6 | Unqualified and grouped-input aggregate contexts |
| R007-11 | R013-3, R013-9, R013-49 | Aggregate context and filter scope; R007 retains window filter scope |

The value-stage sequence remains in R005; R008 owns handler behavior. R023's
boundary description now agrees with the existing R014-16 and R023-21
contracts: quoting is not delivered as part of an ingested value.

Requirement identifiers are permanent references rather than document
positions. The metadata checker accepts existing letter suffixes, rejects
duplicate identifiers, and resolves example citations independently of section
order. It also checks that rules begin with Intent and Boundaries and end
with Errors and Rationale.
