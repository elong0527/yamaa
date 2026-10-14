"""Stable repository diagnostic records shared by focused check owners."""


class ValidationDiagnostic(str):
    """Rendered validation error with stable machine-readable identity."""

    def __new__(
        cls,
        path,
        condition,
        message,
        *,
        context=None,
        span=None,
        rendered=None,
    ):
        location = f" at characters [{span[0]}, {span[1]})" if span is not None else ""
        if rendered is None:
            rendered = f"ERROR: {path}: {message}{location} [{condition}]"
        value = super().__new__(cls, rendered)
        value.path = path
        value.condition = condition
        value.message = message
        value.context = dict(context or {})
        value.span = tuple(span) if span is not None else None
        return value


def validation_diagnostic(path, condition, message, *, context=None, span=None):
    return ValidationDiagnostic(
        path,
        condition,
        message,
        context=context,
        span=span,
    )
