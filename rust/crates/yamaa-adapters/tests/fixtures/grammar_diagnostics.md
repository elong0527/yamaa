# Numeric and aggregate grammar diagnostic truth

`grammar_diagnostics.tsv` holds seven authored original JSON documents (valid
YAML) and complete independent failure reports. Each reads the held CSV bytes
`ID\n1\n` before binding reports its grammar finding. The cases exercise all
three numeric and four aggregate grammar causes, including written lowercase
function spelling, argument count, prohibited comparison and nested reduction.

The independent Python reference `execute_example` produced the reports on
2026-10-07 from the reference source at
`c813b08d98f002e827c2e9fbe1832e965297d24a`. The candidate did not generate them.
Only backend and runtime/engine version identity were normalized to the existing
prototype labels. Source tables, capture accounting, authored paths, node
outcomes and complete contexts remain as observed. No artifact is published.
Existing benchmark expected outputs are unchanged.

Rust and both installed hosts compare these complete reports, retain failed
results after releasing the compiled handle and require repeated saves to fail
without publication or recapture. These are private original-document tests,
not qualification of the replacement public facade or an inventory promotion.
