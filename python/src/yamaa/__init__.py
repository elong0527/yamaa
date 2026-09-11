"""Python helpers for the YAMAA clinical data specification."""

from importlib.metadata import version as _distribution_version

__version__ = _distribution_version("yamaa")

__all__ = ["__version__"]
