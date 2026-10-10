# Producer metadata admission

The #1858 prerequisite admits producing-specification declarations and output
contracts in shared Rust. It does not qualify workflow execution under #1741.

`producer_contract::prepare` accepts an admitted `SpecificationDocument` and
selects stored fields in `output.columns` order. Each stored field needs exactly
one column declaration, its declared logical type, and a nonblank label. Internal
columns omitted from the projection do not enter the stored contract. Empty,
duplicate, undeclared and multiply declared selections retain ordered REQ-0534
causes. Header and typed-field comparison use exact names, cardinality, order and
logical types, with portable REQ-0535 findings. They do not inspect values or
infer types.

`producer_admission::prepare` compiles consumer vocabulary against supplied
producer contracts. A declared `schema` requires exactly one metadata candidate;
missing, duplicate, undeclared, contradictory and self-dependent candidates fail.
Inline `types`, including empty and explicit-null declarations, fail REQ-0523 even
when they agree with the producer. Resolved consumer and producer artifact
identities must agree. The compiler retains declaration order, producer schema
identity, artifact identity, and separate authored origins for the schema link,
consumer artifact path and producer output path.

The result is sealed metadata. Its compiler representation is private and it has
no binding or compiled-plan accessor. `inputs()` distinguishes producer metadata
from external source declarations. Ordinary `PreparedSpecification::prepare`
and ordinary engine/project checking continue to reject `schema`. The engine's
`CheckedProducerMetadata::execution_capability` returns explicit Unsupported
before a caller can obtain the check/build capability. This boundary accepts no
study, activation, runtime callback or publication port.

The adapter accepts already prepared documents and supplied resolved location
facts. It retains exact entry/parent bytes and the contributing layers. Producers
must use the same captured root closure; module identities and bytes are compared
directly. Raw document bytes, schema bytes/module counts, model nodes, identities,
metadata text, candidates, selected fields and compiler resources have aggregate
admission limits. Authored diagnostics use original spellings, including when
inheritance rebased executable paths. Output is an atomic top-level field in the
existing shared layer composition.

Resolved location facts grant no filesystem authority. This prerequisite neither
resolves nor verifies them through native resources, and it does not change the
approved root/fallback policy. #1741 must connect the closed native resource and
graph preparation boundary, validate recursive dependencies, compile producer
execution, activate before study reads, execute each producer once, serialize its
rounded output, ingest held serialized bytes in each consumer, retain failures,
and publish only through explicit successful save in both installed hosts.

The independent workflow truth stays 1.234 and 2.345 -> stored CSV 1.23 and 2.35
-> two consumer references producing 2.46 and 4.70. Component admission tests are
not a shared execution or release qualification claim. No study expected files,
producer artifacts, content digests, numerical policy or submission implementation
are changed here.
