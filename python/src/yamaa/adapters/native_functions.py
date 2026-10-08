"""Explicit optional native callbacks with known temporal-result compatibility."""

from __future__ import annotations

from collections.abc import Callable
from importlib import import_module

from yamaa.models.values import DateTimeValue, DateValue


def invoke_function(request: str, callback: Callable[..., object]) -> str:
    """Invoke one explicit function, preserving known temporal result precision.

    The native engine owns signature, argument and result checks. This facade
    only bridges the reference's designated DateValue/DateTimeValue results to
    owned core temporals; other results retain native exact-type admission.
    Artifact activation and dataset/backend dispatch are separate capabilities.
    """
    yamaa_native = import_module("yamaa._native")

    return yamaa_native.invoke_function(request, adapt_callback(callback))


def adapt_callback(callback: Callable[..., object]) -> Callable[..., object]:
    """Capture one callable and preserve designated temporal result representations."""
    yamaa_native = import_module("yamaa._native")

    if not callable(callback):
        raise TypeError("callback must be callable")
    make_temporal = yamaa_native._temporal_result

    def adapted(**kwargs: object) -> object:
        """Call once; keep callback errors separate from result representation errors."""
        value = callback(**kwargs)
        if not isinstance(value, (DateValue, DateTimeValue)):
            return value
        try:
            kind = "date" if isinstance(value, DateValue) else "datetime"
            fields: tuple[object, ...] = (value.year, value.month, value.day)
            if isinstance(value, DateTimeValue):
                fields += (value.hour, value.minute, value.second)
            precision = value.collected_precision
        except Exception:  # noqa: BLE001 - known-model result admission, not callback code
            # A forged model or subclass accessor may fail. Record an invalid
            # returned representation; never retry the callback or convert this
            # secondary failure into a callback-raised condition. Controls escape.
            return make_temporal(None, None, None)
        return make_temporal(kind, fields, precision)

    return adapted
