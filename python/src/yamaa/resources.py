"""Host resource protocol for domain inputs, metadata and explicit Save.

Authored paths keep the specification's local spelling. A resolver maps each
declaring identity and path to its own immutable local, URI or memory identity.
Credentials stay inside the callback and are never part of retained provenance.
"""

import posixpath
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from typing import Literal
from urllib.parse import urlsplit


@dataclass(frozen=True)
class ResourceRequest:
    operation: Literal["inspect", "capture", "target", "stage", "commit", "abort"]
    kind: Literal["metadata", "input", "output"]
    declaring: str
    written: str
    reference: str | None = None
    data: bytes | None = None


@dataclass(frozen=True)
class ResourceResult:
    reference: str
    data: bytes | None = None
    frame: object | None = None


@dataclass(frozen=True)
class ResourceProvenance:
    declaring: str
    written: str
    kind: str
    reference: str
    representation: Literal["bytes", "frame"]


class ResourceFailure(Exception):
    """An explicitly classified resource failure; messages are never inspected."""

    def __init__(
        self,
        reason: Literal[
            "missing", "changed", "cancelled", "rejected", "publication", "cleanup"
        ],
    ):
        if reason not in {
            "missing",
            "changed",
            "cancelled",
            "rejected",
            "publication",
            "cleanup",
        }:
            raise ValueError("unknown resource failure")
        super().__init__(reason)
        self.reason = reason


class ResourceResolver:
    """Adapt one callback to the native resource ports.

    Inspect and Target return identities without reading study bytes. Capture
    returns immutable bytes and the same identity on reuse. Stage creates a
    private provisional object and returns its token; Commit publishes it
    atomically; Abort removes it after a failed or interrupted publication.
    Abort must also accept the target identity if Stage raised before returning.
    """

    def __init__(self, callback: Callable[[ResourceRequest], ResourceResult]):
        self._callback = callback
        self._pending: BaseException | None = None
        self._provenance: dict[tuple[str, str, str], ResourceProvenance] = {}

    def _session(self):
        return ResourceResolver(self._callback)

    @property
    def provenance(self) -> tuple[ResourceProvenance, ...]:
        """Original callback capture identities, in acquisition order."""
        return tuple(self._provenance.values())

    def _dispatch(self, values):
        try:
            reply = self._callback(ResourceRequest(**values))
            if (
                not isinstance(reply, ResourceResult)
                or not isinstance(reply.reference, str)
                or not reply.reference
                or "\x00" in reply.reference
            ):
                raise ResourceFailure("rejected")
            if reply.data is not None and not isinstance(reply.data, bytes):
                raise ResourceFailure("rejected")
            if values["operation"] == "capture":
                if (reply.data is None) == (reply.frame is None):
                    raise ResourceFailure("rejected")
            elif reply.data is not None or reply.frame is not None:
                raise ResourceFailure("rejected")
            if values["operation"] == "capture":
                key = (values["declaring"], values["written"], values["kind"])
                self._provenance.setdefault(
                    key,
                    ResourceProvenance(
                        *key,
                        reply.reference,
                        "frame" if reply.frame is not None else "bytes",
                    ),
                )
            return {
                "reference": reply.reference,
                "data": reply.data,
                "frame": reply.frame,
                "failure": None,
            }
        except ResourceFailure as error:
            return {"failure": error.reason}
        except BaseException as error:
            # Preserve the exact exception and traceback across the Rust boundary.
            # Cleanup still calls the callback before this owner propagates it.
            if self._pending is None:
                self._pending = error
            return {
                "failure": "cancelled"
                if isinstance(error, (KeyboardInterrupt, SystemExit))
                else "rejected"
            }

    def _raise_pending(self):
        error, self._pending = self._pending, None
        if error is not None:
            raise error

    def _retain_error(self, error: BaseException):
        if self._pending is None:
            self._pending = error


class MemoryResolver(ResourceResolver):
    """Deterministic immutable input snapshots and atomic in-memory outputs."""

    def __init__(
        self, objects: Mapping[str, bytes] | None = None, *, prefix: str = "memory:/"
    ):
        self.prefix = prefix.rstrip("/") + "/"
        self.objects: dict[str, ResourceResult] = {}
        self.outputs: dict[str, bytes] = {}
        self.staging: dict[str, tuple[str, bytes]] = {}
        self.cancelled = False
        self._next = 0
        for key, data in (objects or {}).items():
            self.put(key, data)
        super().__init__(self._request)

    def put(self, key: str, data: bytes, *, reference: str | None = None):
        """Replace an object version; existing attempts refuse the new identity."""
        if not isinstance(data, bytes):
            raise TypeError("resource data must be immutable bytes")
        self._next += 1
        self.objects[key] = ResourceResult(
            reference or f"{self.prefix}{key}#v{self._next}", data
        )

    def put_frame(self, key: str, frame, *, reference: str | None = None):
        """Retain an immutable Arrow-compatible frame without IPC serialization."""
        self._next += 1
        self.objects[key] = ResourceResult(
            reference or f"{self.prefix}{key}#v{self._next}", frame=frame
        )

    def _key(self, declaring: str, written: str) -> str:
        if written.startswith(self.prefix):
            return urlsplit(written).path.lstrip("/")
        if declaring.startswith(self.prefix):
            declaring = declaring[len(self.prefix) :].split("#", 1)[0]
        return posixpath.normpath(
            posixpath.join(posixpath.dirname(declaring), written)
        ).lstrip("/")

    def _request(self, request: ResourceRequest) -> ResourceResult:
        if request.operation == "abort":
            self.staging.pop(request.reference, None)
            # Stage can raise after creating an object and before returning its token.
            for token, (target, _) in list(self.staging.items()):
                if target == request.reference:
                    del self.staging[token]
            return ResourceResult(request.reference or "aborted")
        if self.cancelled:
            raise ResourceFailure("cancelled")
        key = self._key(request.declaring, request.written)
        if request.operation in {"inspect", "capture"}:
            try:
                result = self.objects[key]
            except KeyError:
                raise ResourceFailure("missing") from None
            if request.reference is not None and request.reference != result.reference:
                raise ResourceFailure("changed")
            return ResourceResult(
                result.reference,
                result.data if request.operation == "capture" else None,
                result.frame if request.operation == "capture" else None,
            )
        if request.operation == "target":
            return ResourceResult(self.prefix + key)
        if request.operation == "stage":
            if request.reference is None or request.data is None:
                raise ResourceFailure("rejected")
            self._next += 1
            token = f"stage:{self._next}:{request.reference}"
            self.staging[token] = (request.reference, request.data)
            return ResourceResult(token)
        if request.operation == "commit":
            try:
                target, data = self.staging.pop(request.reference)
            except KeyError:
                raise ResourceFailure("publication") from None
            self.outputs[target] = data
            return ResourceResult(target)
        raise ResourceFailure("rejected")


class SyntheticRemoteResolver(MemoryResolver):
    """Versioned synthetic object-store identities; no service or credentials."""

    def __init__(self, objects: Mapping[str, bytes] | None = None):
        super().__init__(objects, prefix="object://synthetic/")


__all__ = [
    "MemoryResolver",
    "ResourceFailure",
    "ResourceProvenance",
    "ResourceRequest",
    "ResourceResolver",
    "ResourceResult",
    "SyntheticRemoteResolver",
]
