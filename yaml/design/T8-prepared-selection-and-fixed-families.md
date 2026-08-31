# T8 Design: prepared selection and fixed families

This design closes two study-structure questions without adding another schema
construct.

## Prepared record selection

A decision that produces a value, its reason, and the supporting record has an
independent data contract. `adam-adrs-best-response-selection` prepares every
assessment with:

- `BORCAT`, the response category that assessment can support;
- `BORPRI`, the category's clinical priority; and
- `BORSEQ`, the subject-level order by priority, date, and assessment sequence.

The downstream BOR specification reads this materialized dataset as an
ordinary source and selects the record where `BORSEQ = 1` with an ordinary
record lookup. The priority is independently testable, and the category and
date cannot drift because they remain fields of one selected record.

The example suite represents the upstream artifact as the downstream input
fixture. Cross-specification execution and materialization are pipeline
orchestration and are not inferred from paths.

For a same-row decision, `adam-adrs-composite-response` uses one internal
`RULE` column and maps both its value and reason from that result. No lookup
selection language is needed.

## Fixed numbered families

Numbered column families remain fixed at authoring time. Letting source data
create columns would turn a loud dictionary/specification mismatch into a
silent, data-dependent artifact schema. When the dictionary grows, the new
member is declared explicitly and the specification is reviewed again.

The positional suffix records the relationship among family members. A study
may add metadata to make that convention checkable, but the derivation schema
does not infer the relationship or make the column list data-dependent.
