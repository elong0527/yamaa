"""Open and inspect single components beneath retained directory descriptors."""

from __future__ import annotations

import os
import stat
from pathlib import Path

__all__ = ["ensure_available", "is_link", "open_directory", "open_file", "stat_child"]

if os.name == "nt":
    from yamaa.io._windows import open_directory, open_file, stat_child
else:

    def open_directory(path: str | Path, *, dir_fd: int | None = None) -> int:
        return os.open(
            path,
            os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | getattr(os, "O_CLOEXEC", 0),
            dir_fd=dir_fd,
        )

    def open_file(path: str, *, dir_fd: int) -> int:
        return os.open(
            path,
            os.O_RDONLY
            | os.O_NOFOLLOW
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NONBLOCK", 0),
            dir_fd=dir_fd,
        )

    def stat_child(path: str, *, dir_fd: int) -> os.stat_result:
        return os.stat(path, dir_fd=dir_fd, follow_symlinks=False)


def ensure_available() -> None:
    if os.name == "nt":
        return
    if (
        os.open not in os.supports_dir_fd
        or os.stat not in os.supports_dir_fd
        or os.stat not in os.supports_follow_symlinks
        or any(not hasattr(os, name) for name in ("O_DIRECTORY", "O_NOFOLLOW"))
    ):
        raise RuntimeError("component-safe project resource resolution is unavailable")


def is_link(status: os.stat_result) -> bool:
    """Reject Windows junctions and other reparse points as well as symlinks."""
    return stat.S_ISLNK(status.st_mode) or bool(
        getattr(status, "st_file_attributes", 0)
        & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    )
