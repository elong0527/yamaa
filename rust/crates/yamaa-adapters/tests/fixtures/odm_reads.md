# Independently authored ODM reads

These fourteen literal reports and CSV artifacts follow REQ-1265–1278.
Eight separate complete preflight failures in `odm_preflight.tsv` also cover
REQ-1295.
They were authored independently of the new Rust evaluator; no native output
generated their expectations. Historical benchmark expected files are unchanged.

The controls exercise implicit-key feeding records, missing items, all three
qualifiers, source-only predicates, ASCII field binding, ignored vendor fields,
group scope over the original relation, record occurrence scope, column overrides,
strict repeated-record failure even when values differ, complete identifying-field
differences, row and column ambiguity contexts, required identifier counts and
storage coordinates, missing fields, and ambiguous stored names. Eight additional
controls require context, unknown input, vendor predicate, types, and ordinal
failures to retain their complete authored diagnostics before any source read.
Core source-interpretation controls separately carry `odm` spellings in dictionary
data keys and item-like literal values. The preflight scan must preserve ordinary
source admission for those expressions. Mapping execution belongs to its own
migration slice and is combined during installed inventory qualification.

Every case compares the full report, retained source table, source read ledger,
diagnostics, and artifact bytes. Failed results cannot publish; successful results
publish twice without another capture.
