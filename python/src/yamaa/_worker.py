"""The worker the standalone `yamaa` command line runs its Python project
functions through: `python -m yamaa._worker`.

The binary runs on no interpreter, so a run that calls a project function
starts this worker in the interpreter whose packages the project's lock
names, and talks to it over standard input and output, one request line and
one reply line at a time. The worker resolves each callable, runs each call
and reports what is installed with the helpers of `_locked_functions`, the
ones the package's own `run()` uses, so a function behaves as it does there.
The protocol is the one `rust/crates/yamaa-cli/src/worker.rs` describes:
tab-separated fields, percent-encoded text, and typed values.

Standard output carries the protocol, so `sys.stdout` is moved to standard
error before any project code runs: a function that prints reaches the
terminal, as its warnings do, and corrupts nothing.
"""

from __future__ import annotations

import datetime
import os
import sys
from importlib import metadata
from urllib.parse import quote, unquote

from yamaa import _locked_functions as locked
from yamaa import _native

# A byte of these, or a letter or digit, spells as itself.
_UNRESERVED = "-._~"


class Rejected(Exception):
    """A result that is no scalar the protocol spells: the engine's
    `invalid_function_result`."""

    def __init__(self, reason: str, returned: str | None = None):
        super().__init__(reason)
        self.reason = reason
        self.returned = returned


def encode(text: str) -> str:
    return quote(text, safe=_UNRESERVED)


def decode(text: str) -> str:
    return unquote(text, errors="strict")


def decode_value(field: str):
    """The Python value of one argument, as the in-process port gives it."""
    if field == "-":
        return None
    kind, _, text = field.partition(":")
    if kind == "s":
        return decode(text)
    if kind == "i":
        return int(text)
    if kind == "f":
        return float(text)
    if kind == "b":
        return text == "true"
    if kind == "d":
        return datetime.date.fromisoformat(text)
    if kind == "t":
        return datetime.datetime.strptime(text, "%Y-%m-%dT%H:%M:%S")
    if kind == "h":
        return datetime.time.fromisoformat(text)
    raise ValueError(f"the protocol spells no value {field!r}")


def encode_value(value) -> str:
    """One result as the protocol spells it, admitting exactly what the
    in-process port admits: None, the exact built-in numbers and dates,
    and text; anything else is rejected with the reason."""
    if value is None:
        return "-"
    if type(value) is bool:
        return "b:true" if value else "b:false"
    if type(value) is int:
        if not -(2**63) <= value < 2**63:
            raise Rejected("a returned integer exceeds 64 bits")
        return f"i:{value}"
    if type(value) is float:
        return f"f:{value!r}"
    if isinstance(value, str):
        return "s:" + encode(value)
    # strftime pads no year on every platform; the protocol spells four digits.
    if type(value) is datetime.datetime:
        if value.tzinfo is not None or value.microsecond:
            raise Rejected(
                "a returned datetime carries a zone or a fraction of a second"
            )
        return (
            f"t:{value.year:04}-{value.month:02}-{value.day:02}"
            f"T{value.hour:02}:{value.minute:02}:{value.second:02}"
        )
    if type(value) is datetime.date:
        return f"d:{value.year:04}-{value.month:02}-{value.day:02}"
    if type(value) is datetime.time:
        if value.tzinfo is not None or value.microsecond:
            raise Rejected("a returned time carries a zone or a fraction of a second")
        return f"h:{value.hour:02}:{value.minute:02}:{value.second:02}"
    raise Rejected("a binding returned a value of no scalar type", type(value).__name__)


def _version(distribution: str) -> list[str]:
    try:
        version = locked.installed_version(distribution)
    except metadata.PackageNotFoundError:
        return ["missing"]
    if isinstance(version, str):
        return ["version", encode(version)]
    return ["unreadable"]


def _handle(fields: list[str], handles: dict, read_line) -> list[str]:
    """The fields of the `ok` reply to one request, or raise."""
    request, arguments = fields[0], fields[1:]
    if request == "info":
        return [encode(_native._CORE_VERSION)]
    if request == "providers":
        modules = [decode(module) for module in arguments]
        providers = locked._installed_providers(modules)
        return [
            encode(module)
            + "="
            + ",".join(encode(name) for name in providers.get(module, ()))
            for module in modules
        ]
    if request == "standard_library":
        return ["true" if locked.is_standard_library(decode(arguments[0])) else "false"]
    if request == "version":
        return _version(decode(arguments[0]))
    if request == "environment":
        return [
            encode(key) + "=" + encode(value)
            for key, value in locked.marker_environment().items()
        ]
    if request == "integer_digits":
        return [str(locked.integer_digits())]
    if request == "bind":
        call = decode(arguments[0])
        parameters = [decode(name) for name in arguments[2:]]
        target = locked.resolve_callable(call, parameters)
        handle = len(handles) + 1
        handles[handle] = target
        return [str(handle)]
    if request == "invoke":
        target = handles[int(arguments[0])]
        values = {}
        for pair in arguments[1:]:
            name, _, value = pair.partition("=")
            values[decode(name)] = decode_value(value)
        return [encode_value(target(**values))]
    if request == "batch":
        target = handles[int(arguments[0])]
        rows, count = int(arguments[1]), int(arguments[2])
        columns = {}
        for _ in range(count):
            name, *values = read_line().split("\t")
            columns[decode(name)] = [decode_value(value) for value in values]
        return [
            encode_value(
                target(**{name: column[row] for name, column in columns.items()})
            )
            for row in range(rows)
        ]
    if request == "exit":
        return []
    raise ValueError(f"the protocol has no request {request!r}")


def main() -> None:
    # The protocol's own copy of standard output; project code sees
    # standard error in its place.
    protocol = os.fdopen(os.dup(1), "w", encoding="utf-8", newline="\n")
    sys.stdout = sys.stderr
    requests = sys.stdin

    def read_line() -> str:
        line = requests.readline()
        if not line:
            raise EOFError("the command line ended")
        return line.rstrip("\r\n")

    handles: dict = {}
    while True:
        try:
            line = read_line()
        except EOFError:
            break
        fields = line.split("\t")
        try:
            reply = ["ok", *_handle(fields, handles, read_line)]
        except KeyboardInterrupt:
            reply = ["interrupt"]
        except Rejected as rejected:
            reply = [
                "rejected",
                encode(rejected.reason),
                "-" if rejected.returned is None else encode(rejected.returned),
            ]
        except Exception as error:  # noqa: BLE001 - every exception is the function's
            reply = ["error", encode(type(error).__name__), encode(str(error))]
        protocol.write("\t".join(reply) + "\n")
        protocol.flush()
        if fields[0] == "exit":
            break


if __name__ == "__main__":
    main()
