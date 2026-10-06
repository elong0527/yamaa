\name{interpret_schema}
\alias{interpret_schema}
\title{Interpret an experimental decoded schema snapshot through shared Rust}
\usage{interpret_schema(request)}
\arguments{\item{request}{One unclassed, nonmissing JSON character string without attributes.}}
\value{Owned outcome JSON. Malformed transport raises an R error after the
native call returns. Schema findings, unsupported shapes and resource refusals
remain explicit data outcomes.}
\description{Processes schema/1 decoded modules and a query batch through the
shared Rust schema interpreter. The service validates documents and descriptors,
normalizes declared defaults and shorthand, and reports the first matching type
member. The expand_windows operation takes a normalized composed document and
Boolean strict flag, returning independent inline settings and origin/reference
links. Non-strict expansion precedes host inheritance pruning; strict expansion
rejects surviving unknown names and removes root definitions. Both modes validate
unused definitions, and share the batch storage/work quotas. The normalize_layer
operation admits one authored contribution before any normalization, collecting
ordered field findings without publishing a partial document. Missing fields
and column defaults remain deferred; optional null clears and original input
origins survive. Supplied non-column values must be complete. The compose_layers
operation consumes admitted normalized layers in contribution order, preserving
keyed declaration order, column patches, clearing rules and written provenance.
Column defaults are materialized after composition. Invalid results can include a
context_document whose node indices are used only to render diagnostic values;
it is not an accepted specification. Parent traversal, path rebasing, pruning and
complete validation remain caller responsibilities. No Python interpreter, host
callbacks or source-data reads are involved.}
\details{The request limit is 8,388,608 UTF-8 bytes, the response limit is
16,777,216 bytes and at most 256 queries share one validation/normalization
budget. Integer nodes carry canonical decimal strings; JSON numbers cannot
substitute for them. Floating nodes carry 16 lowercase
hexadecimal binary64 bits and must be finite; decimal numbers cannot replace
those bits. Module loading, safe include resolution and YAML 1.2
decoding remain caller responsibilities. This experimental service is not a
complete R specification/workflow API and does not grant execution support.
Compound diagnostic labels use pinned Unicode 18.0.0 printability; compatibility
with older host Unicode versions remains under qualification. Normalization
that would create a non-scalar mapping key is explicitly unsupported.}
