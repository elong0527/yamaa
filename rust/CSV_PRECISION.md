# Shared CSV display precision

The original-document compiler retains `output.decimals` as an optional canonical
integer of arbitrary width. Schema admission rejects non-integer declarations.
Output-declaration checks preserve their existing post-execution validation
boundary: negative precision reports REQ-0744 `invalid_field_type`, and nonnegative
precision on Parquet reports REQ-0762 `decimals_not_applicable`. An earlier
ingestion, derivation, key or verification failure keeps its original cause.

CSV serialization uses the exact binary64 fraction, scales its magnitude and
rounds a genuine tie away from zero. It writes exactly the declared number of
fractional digits, omits the decimal point for zero places, and omits the sign of
a rounded zero. In particular, `0.125` becomes `0.13`, `-0.125` becomes `-0.13`,
and binary64 `2.675` becomes `2.67` at two places. The numeric function's near-tie
interval is separate; CSV does not apply it or convert the rounded decimal back
to binary64. No host formatter or math function supplies the rounding policy.

The codec changes artifact bytes only. Retained tables, dependent calculations,
predicates, verifications and observations use the original values. Without
precision, ordinary shortest positional float text remains in force. Integer,
text, temporal, missing and collected-empty fields keep the existing CSV profile.
Parquet always retains unrounded values and refuses a precision declaration.

Trusted compiler quotas charge integer spellings before cloning metadata. For a
nonempty float projection, the codec charges the minimum declared field width
against the output budget before reading any cell. Each spelling is also charged
before final text allocation. Exact integer intermediates are bounded independently
of declared precision: a finite binary64 needs at most 1,074 fractional decimal
places, so larger declarations append zeros. A quota refusal is resource
exhaustion, not a language maximum. Header-only and non-float projections do not
allocate unused precision. Encoding and report preparation precede explicit save;
failure grants no publication and no partially prepared artifact escapes.

The independent installed Python/R witnesses compare exact CSV bytes, unrounded
tables and calculations, a verification over the unrounded value, 5,000-place
output, invalid/Parquet declarations, quotas and failed-save gates. Shared tests
also exercise adjacent binary64 tie values, subnormals, byte ceilings, zero cell
reads on early refusal and original publication failures. Qualification evidence
is recorded in [the #1860 record](planning/1860-qualification.md).

This closes the CSV precision prerequisite only. #1741 still owns graph
preparation, producer-once execution, activation before data, rounded producer
serialization before consumer ingestion, retained workflow failures and explicit
workflow publication. Its unchanged independent truth remains
`1.234/2.345 -> CSV 1.23/2.35 -> consumer 2.46/4.70`.
