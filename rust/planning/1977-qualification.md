# Aggregate relation implementation evidence

This implementation advances [#1977](https://github.com/elong0527/yamaa/issues/1977).
It does not close that work package or establish the public benchmark ratchet.

The shared compiler admits the three REQ-0467 contexts: the current grouped
input, a qualified relation matched to a constructed row, and constructed
output rows partitioned by declared output columns. A qualifier equal to the
primary input still selects the complete donor relation before matching. An
ungrouped row aggregate receives the existing invalid-context finding required
by REQ-0508, rather than an unsupported-language refusal. The grouped form
retains its enclosing input group and rejects a second local grouping
declaration.

The compiler reuses the aggregate grammar and the scalar numeric compiler.
Reducer arguments and the surrounding arithmetic retain the complete authored
expression and its original spans. Every reducer evaluates before a lazy outer
numeric call. Bare reducer operands retain their exact logical types; numeric
operand expressions use the existing PortableLibmV1 route. Ordered SUM and
MEAN keep the existing checked fold. MIN/MAX preserve exact integer ordering
above 2^53 and the first value on an exact tie. ONLY counts records including
missing values; an empty group remains distinct from a nonempty all-missing
COUNT group.

Compiled aggregate metadata is bounded before cloning. The 16 MiB aggregate
metadata ceiling covers the original expression/path and each retained numeric
operand or result plan. Parser limits, metadata limits and key-list admission
limits remain explicit preparation refusals, rather than invented normative
conditions.

Qualified relation selection completes current-row match expressions before
reading donor keys. Its source predicate visits only matching donor records.
This differs from the retained intermediate source-selection order; the two
routes share match-expression compilation and execution without silently
changing each other's read order. Constructed output reductions borrow the
complete rows already constructed, and each column's values are completed
before that column is published to later derivations.

Typed aggregate findings cover REQ-0501 (row and column ONLY failures),
REQ-0503 (bare fields outside the grouping grain), REQ-0511 (incomparable
extrema), REQ-0507 (output grouping), REQ-0142 (grouped-row local keys),
REQ-0504 (mixed relations), REQ-0141 (key references), REQ-0004 (explicit key
types) and REQ-0140 (empty explicit keys). Existing scope, inferred-key type
and no-applicable-key findings retain their established requirements. The
registry tests independently reach every new cause and compare its complete
context. Neither requirements nor expected benchmark outputs are generated
from the implementation under test.

An ONLY failure retains the authored row identity and complete group object,
including when a grouped row later encounters a column failure. Output key
identity follows the established column failure route. Group identity currently
reads the first member's immutable normalized key cells when projecting the
failure; the opaque-read test records those accesses. This slice does not
claim retirement of the ownership probes or a broader no-reread guarantee.

The independent `correlated_aggregates.tsv` controls compare complete public
reports and output CSV, with literal expected conditions and values. They cover
all three contexts, exact extrema, all-missing and empty counts, full ONLY
context, mixed relations, grouping grain, grouping context, key references and
types, scalar/list key shorthand, repeated shorthand names and both empty key
shapes. Existing stored specifications retain their historical expected bytes.
The same fixture is staged into fresh Python and R source packages.

Rust engine controls additionally observe donor and current-row read prefixes,
matched-only filtering, eager ONLY evaluation and collection of every operand
before a potential earlier fold overflow. A later opaque operand error remains
the original table access error rather than becoming an arithmetic failure.

Aggregate `derive`, row-relative `between`, correlated predicates and reads
through named intermediates remain outside this slice. Their declarations
continue to report explicit unsupported features. The #1974 inventory must
be rerun against both installed hosts, the passing set ratcheted, and the
exact final native matrix plus combined-main audit completed before #1977 can
close. Superseded source checks and packages do not qualify a rewritten head.
