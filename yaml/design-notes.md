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

The former numbered-file layout has been replaced by semantic blocks and
global requirement IDs. See the [migration record](rule-migration.md) for
cutover decisions and the [requirement index](rules/reference/requirements.md)
for the complete legacy-to-canonical mapping. No forwarding contracts remain
in the normative text.
