# Lossless R host scalars

The optional yamaanative package provides designated scalar representations for
existing logical int and str values. They close two host-storage gaps before R
callbacks: R integer reserves its i32 minimum for NA and R double cannot represent
every i64, while R character cannot contain embedded NUL. These classes do not
add a logical type, reinterpret missing, or claim installed R callbacks yet.

- `int64(x)` accepts one canonical decimal character value or a non-missing R
  integer and returns `yamaa_int64`. Its payload is one canonical decimal string.
  All i64 values, including INT64_MIN, remain present values. Double input is
  rejected because it may already be rounded.
- `utf8_scalar(x)` accepts raw UTF-8 or one R character value and returns
  `yamaa_utf8`. Its payload is a raw vector representing one logical string,
  including empty text or embedded NUL. `length` reports one logical scalar;
  `utf8_bytes` exposes the exact owned bytes. Missing is separate.

The exact class, storage and scalar shape are validated on every boundary. Extra
attributes, subclasses, generic wrappers and collections are rejected. A forged
class with invalid payload does not bypass validation. `is.na` returns FALSE for
validated present carriers; ordinary logical NA represents missing during scalar
operations. Typed primitive NA and nonfinite doubles normalize to missing.

Arithmetic +, -, *, /, unary signs and comparisons call shared Rust primitives.
There is no vector recycling or Boolean/text-to-number coercion. R `1` is float;
use `1L` or `int64("1")` for an integer operand. Overflow remains an error with
its exact wider decimal diagnostic. Comparisons retain i64 precision against
binary64 rather than narrowing first. R remainder and other operators remain
explicitly unsupported here rather than silently using a different host policy.

`as.character` returns exact integer text or NUL-free UTF-8. NUL-containing text
cannot become R character and requires `utf8_bytes`; failed conversion does not
change the held scalar. `as.double` and `as.integer` convert only when exact and
non-missing. Binary64 conversion is implemented in Rust and returned as bits:
the local R 4.6.1 decimal parser rounded the INT64_MIN text one representable
step away during testing, so host decimal parsing is deliberately excluded.
Scalar operators return designated carriers for int/str, ordinary R double or
logical for float/bool, and logical NA for missing. The carriers are scalars,
not a vectorized dataframe interface; unsupported R generics are not advertised.

The R facade sends tag/raw-payload pairs across the native boundary. Integer
payloads are canonical ASCII; floats carry eight little-endian bytes; strings
are strictly checked UTF-8; bool is exactly one zero/one byte; missing has no
payload. Rust validates these bytes before constructing a string. This avoids
extendr 0.9's R character accessor, which assumes the underlying bytes are UTF-8.
Declared Latin-1 R strings are explicitly decoded before packing. Byte-marked
strings are rejected. Other character bytes are validated as UTF-8 without
machine-locale fallback, replacement, normalization or repair. Payloads have a
fixed 1 MiB cap. Owned raw outputs survive garbage collection and caller mutation.

The older R JSON APIs also use this explicit R encoding policy. Their native
entrypoints take raw bytes, enforce the transport byte budget, then validate
UTF-8 before making a Rust string. Neither R encoding marks nor `enc2utf8` alone
establish that invariant. Installed tests cover malformed sequences through
both the facade and direct native entrypoints, explicit Latin-1, byte-marked
rejection, size-before-encoding precedence and successful calls after errors.

Native helpers return normally before the facade raises an R error and catch
Rust unwinds. They do not promise recovery from process aborts or allocator
exhaustion. No input data becomes source code, a callable name, or a digest.
Core/engine remain independent of R and the adapters add no external dependency.

Installed source-package tests cover i64 extrema, 2^53+1, the i32 NA collision,
checked arithmetic, exact comparisons/conversions, NUL/Unicode/empty text,
non-normalization, explicit R encodings, malformed UTF-8, forged classes, limits
and ownership after garbage collection. Rust tests independently cover the byte
codec and arithmetic facts. R callback integration will use designated scalar
representations alongside REQ-0563 Date and UTC POSIXct; its result admission,
condition translation, thread ownership and full workflow gates remain open.
