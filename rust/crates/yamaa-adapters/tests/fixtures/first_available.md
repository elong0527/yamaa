# Original first_available selection

Eleven independently authored complete reports cover ordered raw-value selection,
driver-backed filtered operands, missing fallback, omitted fallback, empty lists,
static unknown fields in skipped operands, inapplicable scalar filters, distinct
value conflicts and conversion failures. Six successful reports include exact
CSV bytes. Every expected source/output cell, diagnostic, node, read and artifact
was authored independently, then compared with actual reference Python execution.
No candidate output generated the truth.

All operand references and predicates bind before evaluation, including skipped
operands. Execution stops at the first present raw value and converts only that
result. Missing fallback is a literal rather than a handler and records no handler
count. A selected conversion failure does not advance to later sources. Distinct
value conflicts retain the enclosing expression path; static findings retain the
individual operand path. Filters reuse the existing predicate and collection
services. Key-phase, row-template, secondary and intermediate selection operands
remain outside this compiler slice. Existing predicate parity gaps remain open.

The unchanged negative-source-trivial-filter benchmark receives a separate full
report authored from its committed input and REQ-0148 error declaration. Installed
hosts compare the complete report with new and cached captures and retained save
gates. It is private-route evidence; the assisted inventory remains unchanged.
