# Portable math policy for the shared engine

Status: opt-in scalar prototype for [#1585](https://github.com/elong0527/yamaa/issues/1585).
This is an explicit migration policy, not a claim of existing Python bit parity
or completed Python/R dataset execution. The default compiler is unchanged.

## Decision and API

`compile_numeric` keeps `MathPolicy::ReferenceSubset`: EXP, LN and POWER return
`Unsupported` before any resolution. Callers can explicitly select
`MathPolicy::PortableLibmV1` through `compile_numeric_with_policy` to enable those
functions. A compiled plan retains its policy, exposes it through `math_policy`,
and cannot change it during evaluation. Host adapters must eventually record the
selected policy alongside their backend/version metadata.

PortableLibmV1 uses libm 0.2.16 with default features disabled, Rust 1.90.0 and
ordinary binary64 arithmetic without reassociation or fast-math. The default
floating-point environment is assumed; changing rounding modes or enabling
flush-to-zero is outside this prototype's qualified scope. A library, feature,
algorithm or toolchain change requires requalification; any result-bit change
requires a new policy version rather than quietly redefining PortableLibmV1.

The shared core is the intended authority for both host bindings. Routing through
one implementation avoids separately reproducing the host's math algorithms.
Actual installed Python/R expression entry points remain a later FFI gate.

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
PortableLibmV1 explicitly chooses the pinned shared implementation. It does not
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
  exactly matching sample identities, input bits and result bits across all six
  reports. It rejects missingness and signed-zero differences without tolerance.
- This finite sample establishes neither universal portability nor complete
  mathematical accuracy. Independent high-precision accuracy analysis, difficult
  boundary cases and broader POWER domains remain qualification work.
- Before host/dataset exposure, compare both bindings exactly, assess downstream
  predicates/conversion/rounding/CSV changes, decide migration behavior explicitly,
  and qualify all existing benchmark goldens without rewriting their expected truth.
- Decimal rounding, completed-result conversion/handlers, Cargo locking and full
  dataset execution remain separate gates. Default callers stay on ReferenceSubset
  until the corresponding release decisions are made.
