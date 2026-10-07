# Portable math policy for the shared engine

Status: experimental scalar candidate for [#1585](https://github.com/elong0527/yamaa/issues/1585).
The [#1740 decision proposal](https://github.com/elong0527/yamaa/issues/1740) is
unratified. This document describes the existing prototype and qualification
evidence, not approval of production numerical behavior or Python bit parity.
The default compiler is unchanged.

## Current prototype and target release API

`compile_numeric` keeps `MathPolicy::ReferenceSubset`: EXP, LN and POWER return
`Unsupported` before any resolution. Callers can explicitly select
`MathPolicy::PortableLibmV1` through `compile_numeric_with_policy` to enable those
functions. A compiled plan retains its policy, exposes it through `math_policy`,
and cannot change it during evaluation. The bounded `numeric/1` host protocol
also exposes this experimental selection. Those selectors serve qualification;
they are not part of the target public API.

Under #1751/#1757, each locked yamaa release supplies one numerical behavior in
both hosts and every compute context. Public `domain`, `check` and `define`,
specification YAML and environment YAML do not select a policy or backend. The
host lock pins the package release; rollback pins a prior release before a fresh
run. There is no mid-run policy change or semantic fallback. `PortableLibmV1` is
an internal candidate label, not an additional public version argument or
mandatory result field. Qualification reports retain exact release/build/artifact
evidence. Selecting the release behavior still requires the decision in #1740,
followed by implementation and installed qualification before transition.

PortableLibmV1 uses libm 0.2.16 with default features disabled, Rust 1.90.0 and
ordinary binary64 arithmetic without reassociation or fast-math. The default
floating-point environment is assumed; changing rounding modes or enabling
flush-to-zero is outside this prototype's qualified scope. A library, feature,
algorithm or toolchain change requires requalification. A production change to
observable bits, missingness, zero sign, failure order, conversion or handler
behavior requires an explicit compatibility decision, a new yamaa release and
renewed qualification. Experimental candidate labels may distinguish comparisons;
they must not become a public per-run selector.

The shared core is the intended authority for both host bindings. Routing through
one implementation avoids separately reproducing the host's math algorithms.
Installed Python/R scalar probes already exercise the bounded protocol. Shared
original-YAML execution of these functions in every compute context remains a
separate gate; scalar probe evidence cannot close it.

## Semantics preserved

REQ-0422 requires float results. Integers promote to binary64 before domain checks
or math; in particular a large odd integer exponent may round to an even float.
Every argument evaluates in written order; missing does not suppress failures
inside later arguments. After argument evaluation, missing propagates before domain
checks. EXP overflow normalizes to missing, and underflow remains a finite zero.
LN of zero or a negative value reports REQ-0432. POWER rejects zero to a negative
exponent and a negative base with a fractional promoted exponent under REQ-0433.
Signed-zero behavior follows the pinned algorithm. Domain failures identify the
call; inner failures retain their own source span and opaque resolver payloads.

Invalid POWER diagnostics retain the promoted base/exponent as binary64 bits,
including negative zero. Future host transport must decode those finite bits into
the reference's numeric `base` and `exponent` fields, not expose bit strings as a
new public diagnostic shape.

## Deliberate compatibility distinction

REQ-0435 acknowledges last-place differences; REQ-0436 requires shared R/Python
results, and REQ-0438 preserves written association. These do not make differences
from the current Python reference behavior-neutral. #1620 measured hundreds of
one-ULP differences per function, with different counts on supported OS targets.
The PortableLibmV1 experiment uses the pinned shared implementation. It does not
change Python production evaluation, regenerate goldens, enable a dataset backend
or claim that old and new runs will have identical output.

A one-ULP change can affect a dependent predicate, integral conversion, rounding,
verification or canonical CSV field. Legacy comparisons stay exact and retain
every mismatch. A numerical tolerance must not turn those mismatches into passes.
There is no claim here that libm or platform math is universally correctly rounded.

## Evidence and remaining gates

- 38 independent shared cases exercise algebraic identities, domain failures,
  type promotion, signed zero, missingness, range limits and failure order.
  Both reference Python and the opt-in core replay them against written truth.
- Schema-2 assessment reports retain all 30,033 candidate observations and every
  legacy Python mismatch. The probe executes the actual opt-in compiled policy.
- The native CI matrix runs semantic tests and produces reports on Linux x86_64,
  macOS arm64 and Windows AMD64 with Python 3.12/3.14. A separate job requires
  the independent numeric-math-v1 case identities and input bits, then exactly
  matching result bits across all six
  reports. It rejects missingness and signed-zero differences without tolerance.
- This finite sample establishes neither universal portability nor complete
  mathematical accuracy. Independent high-precision accuracy analysis, difficult
  boundary cases and broader POWER domains remain qualification work.
- Before host/dataset exposure, compare both bindings exactly, assess downstream
  predicates/conversion/rounding/CSV changes, decide migration behavior explicitly,
  and qualify all existing benchmark goldens without rewriting their expected truth.
- Decimal rounding is shared by both policies; numeric completed-result conversion
  and handler accounting is now a qualified engine service (31 shared Python/Rust
  cases, CI on all native targets). Normalized specification dispatch, host
  diagnostics, remaining handlers, Cargo locking and full
  dataset execution remain separate gates. Default callers stay on ReferenceSubset
  until the corresponding release decisions are made.


## numeric-math-v1 input specification

`tools/math_corpus.py` independently specifies the probe inputs. This version
contains boundary-0 through boundary-10 and sample-0 through sample-9999 for each
of EXP, LN and POWER. Missing, renamed or extra cases fail the portability check,
even if every report has the same replacement and unchanged counts. Input-bit
drift also fails before any output comparison; expected outputs are not generated.

The boundary `(x, y)` pairs, in order, are `(-0, 1)`, `(0, 2)`, `(1, 0.5)`,
`(-1, minimum subnormal)`, `(709, minimum normal)`, `(710, maximum finite)`,
`(-744, next float below 1)`, `(-746, next float above 1)`, `(maximum finite, 1)`,
`(-maximum finite, 1)`, and the bit pair `(c05098b14e6ba5d0, 4047354b5f3e6095)`.

For generated cases, begin with unsigned 64-bit state 1585. In each iteration
apply XOR with state shifted left 13, then right 7, then left 17, truncating
left shifts to 64 bits. Compute `u = float(state >> 11) / 2^53`, then
`x = u * 1400.0 - 700.0` and `y = u * 200.0 + 0.001` with the written binary64
association. Each pair supplies EXP inputs `(x, +0)`, LN inputs `(y, +0)` and
POWER inputs `(y, x / 100.0)`; unused unary second inputs remain explicit.
Changing this corpus requires an explicit specification update and review.
