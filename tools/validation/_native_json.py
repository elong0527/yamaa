"""Lossless JSON scalar codecs for bounded native transport records."""

import json
import math


def integer(text):
    negative = text.startswith("-")
    digits = text[1:] if negative else text
    result = 0
    for offset in range(0, len(digits), 9):
        chunk = digits[offset : offset + 9]
        result = result * 10 ** len(chunk) + int(chunk)
    return -result if negative else result


def integer_text(value):
    negative = value < 0
    value = abs(value)
    chunks = []
    while value:
        value, chunk = divmod(value, 1_000_000_000)
        chunks.append(chunk)
    if not chunks:
        return "0"
    text = str(chunks.pop()) + "".join(f"{chunk:09d}" for chunk in reversed(chunks))
    return "-" + text if negative else text


class Encoder(json.JSONEncoder):
    def iterencode(self, value, _one_shot=False):
        def number(value):
            if math.isfinite(value):
                return repr(value)
            if not self.allow_nan:
                raise ValueError("non-finite JSON scalar")
            return (
                "NaN" if math.isnan(value) else "Infinity" if value > 0 else "-Infinity"
            )

        encode_string = (
            json.encoder.encode_basestring_ascii
            if self.ensure_ascii
            else json.encoder.encode_basestring
        )
        return json.encoder._make_iterencode(
            {} if self.check_circular else None,
            self.default,
            encode_string,
            self.indent,
            number,
            self.key_separator,
            self.item_separator,
            self.sort_keys,
            self.skipkeys,
            False,
            _intstr=integer_text,
        )(value, 0)


def loads(encoded):
    return json.loads(encoded, parse_int=integer)


def dumps(value, **options):
    return json.dumps(value, cls=Encoder, **options)
