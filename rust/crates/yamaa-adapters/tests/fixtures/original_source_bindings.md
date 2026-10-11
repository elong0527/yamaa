# Original source bindings

Sixteen independently authored original YAML documents, source records, complete
reports, handler counts and exact CSV bytes cover scalar driver and secondary
source bindings. The truth was written from REQ-0075, REQ-0127, REQ-0150-0152 and
REQ-0345/0353-0356. No candidate or reference execution generated the expectations.

Cases cover eligibility before ordered selection, stable first/last ties, explicit
null placement with descending order, empty matches with zero handler counts,
absent variables with a literal or explicit null, present missing values without
absence handling, driver collection selection, lazy intermediate and secondary
first_available operands, duplicate donor records with equal values, and missing
applicable keys, exact int/float comparisons beyond 2^53, and reached/unused
invalid absence replacements with both handler and conversion context. Successful cases are saved twice; every document is run twice
with fresh attempt-local choices and handler counts. Negative results cannot save.

The source.condition wording in issue #1980 has no grammar definition.
`yaml/schema_expression_core.yaml` source_binding_class and REQ-0093-0095/0351-0355
define variable, filter, order_by, keep and absent. Condition belongs to flag_class.
Existing unknown-field schema diagnostics remain authoritative. Single-value
ordered sources and key-phase source options remain explicitly unsupported in
this first slice. Mapping/cut and complete cross-family handler projection remain
open work under #1980; this evidence does not close that work package.
