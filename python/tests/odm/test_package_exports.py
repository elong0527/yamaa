"""Package-root export regression tests for yamaa.odm."""

from __future__ import annotations

from yamaa.odm import ODMError
from yamaa.odm.errors import ODMError as CanonicalODMError


def test_odm_error_is_exported_from_package_root() -> None:
    """Importing ODMError from yamaa.odm yields the canonical class object."""
    assert ODMError is CanonicalODMError


def test_odm_error_is_in_package_all() -> None:
    """The package-root export is advertised through __all__."""
    import yamaa.odm

    assert "ODMError" in yamaa.odm.__all__
