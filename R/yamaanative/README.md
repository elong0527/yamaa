# Optional native installation probe

This is a source template. Stage the shared Rust workspace before building:

```sh
python rust/tools/stage_r_package.py /tmp/yamaa-stage/yamaanative
```

Then run `R CMD build /tmp/yamaa-stage/yamaanative` from a temporary output
directory. See [`rust/README.md`](../../rust/README.md) for prerequisites,
installation tests, and the deliberately limited capability of this package.

## Scalar transport probe

`scalar_round_trip(request)` accepts and returns one owned UTF-8 JSON string.
It decodes to a real core value before encoding the result. It does not execute
specifications, evaluate expressions, call project functions or expose tables.

```r
scalar_round_trip('{"protocol":"scalar/1","value":{"int":"9007199254740993"}}')
```

The envelope has exactly `protocol` and `value`. Protocol `scalar/1` has these
closed, single-key value variants:

| Variant | Payload |
| --- | --- |
| `missing` | JSON null |
| `int` | Canonical decimal string in the full signed i64 range |
| `float` | Exactly 16 lowercase hexadecimal binary64 digits, most significant first |
| `str` | JSON string, including empty text and escaped NUL |
| `bool` | JSON true or false |
| `date` | Object with canonical `text` (`YYYY-MM-DD`) and `precision` (`year`, `month`, `day`) |
| `datetime` | Object with canonical `text` (`YYYY-MM-DDTHH:MM:SS`) and `precision` (`day`, `second`) |

For example, missing is `{"missing":null}` and negative zero is
`{"float":"8000000000000000"}`. Nonfinite float bit patterns normalize to missing
under REQ-0006. Temporal precision survives this transport; this is not the
language's conversion-to-text boundary. An absent source record is not a scalar
and has no variant here. Integers never pass through an R double or NA sentinel.

Unknown/duplicate fields, numeric JSON integers, malformed payloads, invalid
civil fields and unknown versions are rejected. Requests are limited to
1,048,576 UTF-8 bytes before JSON parsing. This is a prototype transport policy,
not a language limit. Returned data is owned; it does not borrow the request.
Invalid input returns normally from Rust before the R facade raises a condition.
No JSON package is required in R. Callers keep the envelope when exact ordinary
R representation is unavailable; lossy conversion is not performed implicitly.

The same probe is `yamaa_native.scalar_round_trip` in Python. Both installed
packages replay 52 independent positive/negative vectors, reuse results after
input release/garbage collection, and recover after invalid requests. This
qualifies copied scalar text only, not Arrow buffer ownership, callbacks,
diagnostic transport, dataset execution, R Windows installation or CRAN safety.
