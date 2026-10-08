# Original filtered source expressions

Thirteen independently authored complete reports and exact CSV bytes cover record
eligibility, empty selections, null predicates, repeated identical raw values,
unqualified/wrong-dataset filters, absent stored fields, repeated identifiers, BETWEEN subjects, unreachable absent fields, predicate grammar and
filters over one completed output value. Every expected cell, finding, source
read, node and artifact was authored independently and then compared with an
actual reference Python execution. Only backend/runtime identifiers differ.
No candidate output generated this truth.

Filters bind after complete ingestion, retain authored paths and inspect every
identifier, including unreachable branches. REQ-0132 keeps eligibility within
the source dataset. REQ-0148 rejects filters over single output/intermediate
values before parsing that inapplicable filter. The existing core predicate and
collection services evaluate eligibility before reading candidate values.
Filtered key assignments, secondary source expressions and source ordering
remain explicitly unsupported before study effects in this bounded slice.
The existing predicate requirement/context gaps remain separate and unqualified.
