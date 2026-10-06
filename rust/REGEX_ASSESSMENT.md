# Portable regex migration assessment

The shared compiler cannot copy the current Python regex binding as its sole
oracle. R022 contract 2.0.0 names ECMA-262 Unicode-pattern semantics, with the
repository's explicit restrictions and normalization. The unchanged Python
binding passes the 43 committed `yaml/conformance/regex.yaml` cases, but those
cases do not establish complete contract coverage.

Twelve additional expectations in
`crates/yamaa-adapters/tests/fixtures/regex_contract.tsv` were authored from
`rules/operations/text.md` and the ECMA-262 pattern algorithms. Ten currently
disagree with Python. Neither engine output generated these expectations;
Node's Unicode-mode regex implementation was used only as a secondary check.
The existing YAML vectors and benchmark goldens remain unchanged.

| Case | Required observation | Existing Python observation |
| --- | --- | --- |
| `^[\S]$` against U+FEFF | No match; U+FEFF is whitespace, including inside a class | Matches the character |
| `^[\S]$` against U+0085 | Matches; U+0085 is outside the declared whitespace set | Agrees |
| `\01` | Invalid Unicode-pattern escape | Accepts an octal escape |
| `(?<=a\|b)c` against `ac` | Fixed-width alternatives are valid; search finds `c` | Rejects every lookbehind alternative |
| `^(a\|(b))\2c$` against `ac` | An unentered capture's backreference matches empty | No match |
| `^\1(a)$` against `a` | Forward reference is valid and initially unset; matches `a` | Rejects the reference |
| `^(?<x>a)\k<x>$` against `aa` | Named backreference matches the capture | Rejects the escape |
| `[]` against `x` | Valid empty character set; no match | Rejects the pattern |
| `[^]` against newline | Valid complemented empty set; matches one scalar | Rejects the pattern |
| `\!` | Invalid identity escape with Unicode mode set | Accepts the punctuation escape |
| `[\d-a]` | Invalid range endpoint | Agrees |
| `(a\|(b))+` against `aba` | The final repetition clears the unentered second capture | Retains `b` from an earlier repetition |

These are semantic mismatches, not permitted comparison tolerances. R022's
explicit rejection of variable-length lookbehind still applies; the equal-width
example does not relax it. Pattern syntax acceptance, selected match and capture
participation all need independent expectations. An empty capture, an unentered
capture, and no match must remain separate observations.

The primary external references are ECMA-262's
[pattern grammar and early errors](https://tc39.es/ecma262/multipage/text-processing.html#sec-patterns),
[BackreferenceMatcher](https://tc39.es/ecma262/multipage/text-processing.html#sec-backreference-matcher),
and [RepeatMatcher](https://tc39.es/ecma262/multipage/text-processing.html#sec-repeatmatcher).
The repository's REQ-0798, REQ-0800, REQ-0802, REQ-0813, REQ-0815, REQ-0817,
and REQ-0822 through REQ-0827 determine how these apply here. No new host dialect
or changed contract version is being substituted.

Run the read-only reference assessment from the locked Python environment:

```sh
uv run --project python --locked python rust/tools/assess_regex_reference.py
```

It reports every expected/observed value, an explicit mismatch count, and
`qualification: not-qualified`. A zero exit code means the report was produced;
it does not mean regex parity passed. This assessment is not a CI waiver or a
fixture writer. The shared Rust implementation must pass the independent cases;
reference disagreements must be corrected or explicitly resolved before release
qualification. A passing sampled comparison cannot establish the whole grammar.

## Bounded core implementation

`yamaa_core::regex::Pattern` now compiles an immutable postorder arena and
provides separate search and full-match entrypoints. Its explicit backtracking
stack preserves written alternatives, greedy/lazy choices and full-match
continuations. Positive lookarounds are atomic, fixed-width lookbehind traverses
backwards, repetitions clear descendant captures, and unset backreferences match
empty. Captures borrow subject slices; an empty capture, an unentered capture and
no match remain distinct. There is no host regex library or new dependency.

Default compilation budgets are 65,536 pattern bytes, 4,096 arena nodes,
256 groups, depth 64, repetition counts up to 1,000,000 and statically fixed
width up to 1,048,576 scalars. Width arithmetic is checked without expanding
repetitions. Callers select budgets, but nesting has an unconditional ceiling
of 64. Matching uses explicit tasks for flat concatenations and repetition;
only bounded group parsing and nested assertions recurse. Capture-dependent
width analysis traverses flat sequences iteratively and recurses only through
grammar-bounded nesting.

Each match has a fresh budget shared by every search position, backtrack and
assertion. Default independent ceilings are 1,048,576 subject bytes, 1,000,000
work units and 1,000,000 cumulative logical state cells. Passing the byte check
does not promise that indexing or matching fits either remaining budget: scalar
indexing alone costs `2 * scalar_count + 1` cells, so the default storage cap
refuses an ASCII subject above 499,999 scalars before matching. Backtracking and
capture/task copies can exhaust budgets on much smaller inputs. Callers can
raise individual limits explicitly; the default policy does not guarantee any
maximum subject size will match under every pattern. Scalar indexing and capture/task copies
are charged before allocation. Cells count logical slots rather than allocator
capacity or process-wide memory. Allocator failure, caller-owned input allocation,
concurrent calls and total process memory are outside this policy. Resource
refusals remain separate from invalid syntax and ordinary no-match.

Unicode and escaped capture-group identifiers use pinned Unicode 18.0.0
ID_Start/ID_Continue tables, with ECMA's explicit dollar, underscore and join
control additions. Fixed-width and braced Unicode escapes decode to the same
name identity; no Unicode normalization or host identifier classification is
used. Duplicate decoded names remain invalid and forward named references still
resolve after parsing. Source diagnostics retain original byte coordinates.
Safe range lookups introduce no dependency or unsafe code. The original licensed
UCD source is retained; reproduction and exhaustive scalar-membership tests
are described in [the data provenance](crates/yamaa-core/unicode/README.md).

Backreference-dependent lookbehind is admitted by a bounded structural width
analysis after reference identities are resolved. The pass follows capture
state in matching order, including reverse lookbehind evaluation, positive
assertion capture commits, negative assertion isolation, and repeat clearing.
Unentered and empty captures both contribute zero reference width; the matcher
continues to distinguish their result values. Alternative capture vectors stay
separate, so a constant combined width is not lost when individual captures
vary between branches. For example, `(?<=\1\2(?:(a)bb|(aa)))c` consumes four
scalars on either alternative even though their literal lengths differ.

Repetition is analyzed symbolically: descendant captures are cleared before
one representative iteration, earlier iteration widths are combined by checked
count arithmetic, and final captures come from the last iteration. Optional
iterations must consume; required empty iterations and the zero-iteration
capture state remain distinct paths. Variable consumed width is `Invalid`;
analysis exhaustion is a resource refusal. Default independent analysis budgets
are 1,000,000 cumulative node visits/comparison units and 1,000,000 cumulative
logical capture/path/observation slots. These bounds apply to the additional
capture-dependent pass; statically fixed lookbehinds skip it even when unrelated
references occur elsewhere, and ordinary parsing retains its existing limits. Logical
slots do not bound allocator capacity, caller memory or concurrent allocations.
A fresh compile can retry with caller-selected budgets.

Width admission remains structural: it does not execute a subject, prove
literal/assertion satisfiability, or exempt nested variable-width assertions
inside zero-count or negative bodies. This preserves the existing grammar
restriction rather than using sampled matches to decide validity. Rust tests
include authored capture, assertion and direction cases, 350 independent integer
count/alternative admission checks, checked arithmetic, analysis exhaustion and
successful retry. The optional [regex/1 service](REGEX_TRANSPORT.md) now provides
an installed boundary for compilation, search and full-match observations.
It does not establish dataset capability or full grammar qualification. The
development JSON-lines probe remains a separate core qualification tool.

`check_regex.py` requires 2,101 independent observations, including all 43
existing cases, twelve authored edges, scalar-set membership and integer-count
truth. Rust tests additionally cover capture/ordering behavior, empty repeated
groups, reversed lookbehind capture order, flat 3,000-scalar patterns, nested
assertions, checked width overflow, budget exhaustion and successful retry.
Native CI runs these checks and preserves the unchanged Python discrepancy report.
The report cannot waive a failing Rust expectation or qualify reference parity.
Braced code-point escapes also obey REQ-0825's scalar-expansion boundary:
all 2,048 surrogate code points are rejected in atoms and character classes,
while neighboring, unassigned and maximum scalar values remain valid. This
repository normalization is checked independently of raw Node Unicode syntax.

A supplemental development comparison covers 496 patterns and 18,848 observations
against Node Unicode mode. Reproduce it with an installed Node executable:

```sh
uv run --project python --locked python rust/tools/compare_regex_node.py --output regex-comparison.json
```

The report records the Node version and every mismatch; even zero differences
retain `qualification: not-qualified`. Node is not a production dependency or
the source of authored expected values. The current exercised sample agrees,
but this is not proof of full regex conformance.

## Remaining integration and release gates

Complete full compiler/matcher conformance, resolve the reference mismatches
above, and integrate the installed service with consuming operations. Predicate
`str_contains` validates its regex during parsing, so
shared predicate compilation needs matching validation/failure order. Compilation
must finish before host data or callback effects and no mid-run fallback is
allowed. Regex-backed schema/verification/text consumers, workflow integration,
current-schema R execution and full release qualification remain open. Python is
still default and `execution_supported=false`.
