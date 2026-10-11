# Declarative rounding compiler

Original YAML `round_half_away_from_zero: {source: VAL, digits: 1}` lowers to
one core `Rounding` declaration and one bound source operand. It does not create
formula text, expression spans or numeric-function diagnostic context. Formula
and declarative rounding call the same integer-places arithmetic primitive.

The shipped schema admits mapping/source/integer fields before the compiler.
For original YAML, malformed digits keep the schema's
`invalid_field_type` / `REQ-0287` finding at the written digits path before
unresolved names or study reads. The retained standalone expression handler's
`REQ-1172` payload condition does not replace schema admission. Known nonnumeric
bare output sources fail core preflight with `REQ-0418`, original source path,
and exactly `source`, `expected` and `actual` context. This also visits nested
case branches, including branches not selected at runtime. Diagnostic paths and
nested traversal have explicit text/depth bounds before diagnostic copies.

Binding reuses core output catalogs, named intermediate plans and row scope
validation. Column operands can read completed outputs, the typed driver or
admitted named intermediate selections. Nested cases and row templates use the
same operation and arithmetic. Row reference availability/group findings retain
the existing binder checkpoint. The existing secondary source expression and
row/intermediate composition compiler gates remain: rounding does not replace
an unsupported driver or intermediate model with a different execution route.

At runtime opaque source failures propagate before value inspection. Missing
returns missing before runtime type checks. A present nonnumeric value retains
the declarative `REQ-0418` field context and cannot invoke an output conversion
handler. Output conversion runs once, after successful rounding, with its
existing recovery and accounting rules. Operand plans are admitted before table
access, with the dataset's existing depth and node budgets.

`schema-round-half-away-from-zero-ties` keeps its original YAML and independent
16-row expected CSV. Public Python and R witnesses compare every output column,
row, type, missing value and saved CSV against that existing truth. The original
negative fixture is checked with its study input removed and its complete error
fields compared. Python semantic fallback imports are forbidden; R runs with
Python absent from PATH. Additional adapter tests exercise typed driver reads,
named intermediates, nested cases, row templates and unselected-branch type
rejection against the same independent numeric truth. These tests qualify this
operator; they do not close the remaining compiler breadth in #1751.
