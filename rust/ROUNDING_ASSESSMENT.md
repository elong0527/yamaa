# Decimal rounding characterization

Status: assessment only. Rust still rejects ROUND_HALF_AWAY_FROM_ZERO under both
math policies. Production Python, normative rules and benchmark expectations are
unchanged. This work identifies decisions needed before implementing rounding.

## Independent interpretation of REQ-0418

The oracle in `tools/assess_rounding.py` treats the promoted finite binary64 input
as its exact rational value. Let `q = 10^-digits` and `s = abs(value) / q`.
Write `s = k + f`, where `k` is an integer and `0 <= f < 1`. Select `k + 1`
when `f >= 1/2 - 2^-26`; otherwise select `k`. Multiply by the exact quantum,
restore the sign, convert once to binary64 and canonicalize zero to positive
zero. A result beyond binary64's finite range is represented as missing, matching
the numeric nonfinite normalization contract. An exception from the reference
helper is separately recorded and never silently normalized by the assessment.

This directly implements the inclusive near-tie interval in REQ-0418. It does not
call Python `round`, a libm power or the production rounding helper. The Python
helper currently adds a binary64 approximation to the tolerance, then uses
Python's decimal ties-to-even rounding. Floating-point addition can land exactly
on a tie or cross the specified near-tie boundary. The two procedures therefore
need explicit compatibility review; copying the helper is not proof of the rule.

The oracle bounds extreme digit counts before constructing decimal powers:
beyond 340 places the final binary64 is unchanged, and below -309 places every
finite binary64 rounds to zero. The assessed digit range includes both i64 ends.
Arbitrary integer argument validation, missing propagation, resolver ordering,
result conversion, handlers and rendering are outside this scalar assessment.

## Corpus and evidence

`rounding-boundaries-v1` contains 2,936 named cases:

- Adjacent binary64 inputs around exact ties and the lower near-tie boundary,
  using decimal places -308, -100, -2, -1, 0, 1, 2, 15, 16, 100, 308 and 323;
  integral quantum indices 0, 1, 2, 9, 99 and 1,000,001; and both signs.
- Signed zero, minimum subnormals, maximum finite values and promoted i64 ends,
  with extreme digit counts including both i64 ends.
- Finite values from 2,000 iterations of unsigned 64-bit xorshift (seed 1585;
  shifts left 13, right 7, left 17, truncating each left shift to 64 bits),
  interpreted as binary64 bits with a fixed cycle of decimal places.

The code defines every case ID and input. Tests check corpus size/uniqueness,
hand-specified ordinary results, exact inclusive boundary neighbors, signed zero,
range behavior and honest reporting of deliberately incorrect results/exceptions.
The script stores input bits, decimal-text digits, rational results and reference
observations for every case, with a separate complete mismatch list. Outputs are
assessment evidence, not benchmark goldens. Reports are uploaded by all six native
OS/Python CI combinations as `rounding-assessment-*` artifacts.

On local macOS arm64/Python 3.14.7, 2,888 cases agree and 48 differ: 38 result-bit
differences and 10 escaped OverflowError outcomes. For example, the exactly
representable input `0.5 - 2^-26` at zero places is the inclusive near-tie endpoint:
the rule selects 1.0, while adding the tolerance lands exactly on 0.5 and Python's
ties-to-even selects 0.0. At -308 places, rounding maximum finite binary64 escapes
with OverflowError rather than producing a normalized numeric result. Platform
reports remain authoritative observations; local counts are not asserted as
expected production behavior.

Run from the repository root:

```sh
uv run --project python --locked python rust/tools/assess_rounding.py \
  --output /tmp/rounding-assessment.json
```

A successful process validates the assessment, not parity. Reports use
`blocked-by-mismatches` when any case differs and `not-qualified` otherwise.
Before Rust implementation, resolve the reference/rule discrepancy explicitly,
add independently specified regression expectations to both runners, qualify
cross-platform results and assess downstream conversion/CSV consequences. Do not
relax these comparisons with a blanket tolerance or regenerate benchmark truth.
