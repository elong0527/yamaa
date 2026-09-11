"""Errors raised by the ODM importer."""


class ODMError(ValueError):
    """Base class for deterministic ODM import failures."""


class ODMParseError(ODMError):
    """The XML document does not match a supported ODM structure."""


class ODMMetadataError(ODMError):
    """Clinical data cannot be resolved against scoped ODM metadata."""


class ODMArchiveError(ODMError):
    """An archive is unsafe, ambiguous, or outside configured limits."""
