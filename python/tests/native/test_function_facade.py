"""The optional facade bridges known models; the native engine owns admission."""

import sys
from types import SimpleNamespace

import pytest

from yamaa.adapters.native_functions import invoke_function
from yamaa.models.values import DateTimeValue, DateValue


@pytest.fixture
def native(monkeypatch):
    """Expose callback effects and bridge fields without requiring a native build."""
    calls = []
    factories = []

    def factory(*args):
        """Record the exact fields forwarded to native result admission."""
        factories.append(args)
        return args

    def invoke(request, callback):
        """Expose facade execution without interpreting the native request."""
        calls.append(request)
        return callback()

    monkeypatch.setitem(
        sys.modules,
        "yamaa_native",
        SimpleNamespace(invoke_function=invoke, _temporal_result=factory),
    )
    return calls, factories


@pytest.mark.parametrize("precision", ["year", "month", "day"])
def test_date_fields_and_precision(native, precision):
    """Every supported collected precision reaches the closed bridge unchanged."""
    value = DateValue(year=1, month=2, day=3, collected_precision=precision)
    assert invoke_function("request", lambda: value) == (
        "date",
        (1, 2, 3),
        precision,
    )
    assert native[0] == ["request"]
    assert native[1] == [("date", (1, 2, 3), precision)]


@pytest.mark.parametrize("precision", ["day", "second"])
def test_datetime_fields_and_precision(native, precision):
    """Datetime metadata must not be lost through a builtin datetime conversion."""
    value = DateTimeValue(
        year=9999,
        month=12,
        day=31,
        hour=23,
        minute=59,
        second=59,
        collected_precision=precision,
    )
    assert invoke_function("request", lambda: value) == (
        "datetime",
        (9999, 12, 31, 23, 59, 59),
        precision,
    )


def test_subclass_and_unknown_object(native):
    """Match known-model subtype acceptance without duck-typed attribute probes."""

    class SubDate(DateValue):
        pass

    class Unknown:
        def __getattr__(self, name):
            """Fail if result admission probes an unknown object."""
            raise AssertionError("must not inspect unknown returned objects")

    value = SubDate(year=2024, month=2, day=29, collected_precision="month")
    assert invoke_function("request", lambda: value) == ("date", (2024, 2, 29), "month")
    unknown = Unknown()
    assert invoke_function("request", lambda: unknown) is unknown
    assert len(native[1]) == 1


def test_broken_known_model_is_invalid_result(native):
    """Representation extraction failure does not become a callback exception."""
    value = DateValue.model_construct(year=2024, month=2)
    effects = []

    def callback():
        """Record or raise the selected callback outcome without retrying."""
        effects.append(1)
        return value

    assert invoke_function("request", callback) == (None, None, None)
    assert effects == [1]


@pytest.mark.parametrize(
    "error", [ValueError("boom"), KeyboardInterrupt(), SystemExit(3)]
)
def test_callback_exceptions_are_not_swallowed(native, error):
    """The actual callback remains outside the result-adaptation error handler."""

    def callback():
        """Record or raise the selected callback outcome without retrying."""
        raise error

    with pytest.raises(type(error)) as raised:
        invoke_function("request", callback)
    assert raised.value is error
    assert not native[1]


def test_old_native_api_fails_before_callback(monkeypatch):
    """An incompatible installed native version cannot execute callback effects."""
    effects = []
    monkeypatch.setitem(sys.modules, "yamaa_native", SimpleNamespace())
    with pytest.raises(AttributeError):
        invoke_function("request", lambda: effects.append(1))
    assert effects == []


def test_non_callable_is_rejected(native):
    """Never turn a non-callable into a callable facade that fails after admission."""
    with pytest.raises(TypeError, match="callback must be callable"):
        invoke_function("request", "name")
    assert not native[0]
