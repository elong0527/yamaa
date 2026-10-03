"""Windows equivalents of no-follow, directory-relative POSIX file opens.

NtCreateFile resolves one child name relative to a retained directory handle.
FILE_OPEN_REPARSE_POINT opens the entry itself, including junctions, so the
caller can reject it before reading or descending. Absolute paths are used
only when approving a root. Handles become binary CRT descriptors so the
shared resource walker can use os.fstat, os.dup, os.close, and os.fdopen.

https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile
"""

from __future__ import annotations

import ctypes
import errno
import msvcrt
import os
from ctypes import wintypes
from pathlib import Path

_FILE_READ_ATTRIBUTES = 0x0080
_SYNCHRONIZE = 0x00100000
_GENERIC_READ = 0x80000000
_SHARE_ALL = 0x00000007  # FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
_OPEN_EXISTING = 3
_FILE_OPEN = 1
_FILE_DIRECTORY_FILE = 0x00000001
_FILE_SYNCHRONOUS_IO_NONALERT = 0x00000020
_FILE_OPEN_REPARSE_POINT = 0x00200000
_FILE_FLAG_OPEN_REPARSE_POINT = 0x00200000
_FILE_FLAG_BACKUP_SEMANTICS = 0x02000000
_OBJ_CASE_INSENSITIVE = 0x00000040
_INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value


class _UnicodeString(ctypes.Structure):
    """A length-prefixed UTF-16 name passed to the Windows object manager."""

    _fields_ = [
        ("Length", wintypes.USHORT),
        ("MaximumLength", wintypes.USHORT),
        ("Buffer", wintypes.LPWSTR),
    ]


class _ObjectAttributes(ctypes.Structure):
    """The retained parent handle and child name for a native open."""

    _fields_ = [
        ("Length", wintypes.ULONG),
        ("RootDirectory", wintypes.HANDLE),
        ("ObjectName", ctypes.POINTER(_UnicodeString)),
        ("Attributes", wintypes.ULONG),
        ("SecurityDescriptor", wintypes.LPVOID),
        ("SecurityQualityOfService", wintypes.LPVOID),
    ]


class _IoStatusValue(ctypes.Union):
    """The native I/O status union with pointer-sized storage."""

    _fields_ = [("Status", wintypes.LONG), ("Pointer", wintypes.LPVOID)]


class _IoStatusBlock(ctypes.Structure):
    """The completion status and information returned by NtCreateFile."""

    _fields_ = [("Value", _IoStatusValue), ("Information", ctypes.c_size_t)]


_kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
_ntdll = ctypes.WinDLL("ntdll")
_create_file = _kernel32.CreateFileW
_create_file.argtypes = [
    wintypes.LPCWSTR,
    wintypes.DWORD,
    wintypes.DWORD,
    wintypes.LPVOID,
    wintypes.DWORD,
    wintypes.DWORD,
    wintypes.HANDLE,
]
_create_file.restype = wintypes.HANDLE
_close_handle = _kernel32.CloseHandle
_close_handle.argtypes = [wintypes.HANDLE]
_close_handle.restype = wintypes.BOOL
_nt_create_file = _ntdll.NtCreateFile
_nt_create_file.argtypes = [
    ctypes.POINTER(wintypes.HANDLE),
    wintypes.ULONG,
    ctypes.POINTER(_ObjectAttributes),
    ctypes.POINTER(_IoStatusBlock),
    ctypes.c_void_p,
    wintypes.ULONG,
    wintypes.ULONG,
    wintypes.ULONG,
    wintypes.ULONG,
    ctypes.c_void_p,
    wintypes.ULONG,
]
_nt_create_file.restype = wintypes.LONG
_dos_error = _ntdll.RtlNtStatusToDosError
_dos_error.argtypes = [wintypes.LONG]
_dos_error.restype = wintypes.ULONG


def _descriptor(handle: int) -> int:
    """Transfer ownership to the CRT, closing the handle if transfer fails."""
    try:
        return msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY | os.O_NOINHERIT)
    except BaseException:
        _close_handle(handle)
        raise


def _open_child(path: str, dir_fd: int, *, directory: bool, metadata: bool) -> int:
    """Open a child beneath its parent without following reparse points."""
    # RootDirectory must be the only anchor. Reject native separators and
    # traversal or alternate streams that would change that boundary.
    if (
        not path
        or path in (".", "..")
        or any(character in path for character in "\\/:\0")
    ):
        raise OSError(errno.EINVAL, "not a single file component")
    try:
        length = len(path.encode("utf-16-le"))
    except UnicodeEncodeError as error:
        raise OSError(errno.EINVAL, "file component is not valid Unicode") from error
    if length > 65532:
        raise OSError(errno.ENAMETOOLONG, "file component is too long")
    buffer = ctypes.create_unicode_buffer(path)
    name = _UnicodeString(length, length + 2, ctypes.cast(buffer, wintypes.LPWSTR))
    attributes = _ObjectAttributes(
        ctypes.sizeof(_ObjectAttributes),
        msvcrt.get_osfhandle(dir_fd),
        ctypes.pointer(name),
        _OBJ_CASE_INSENSITIVE,
        None,
        None,
    )
    handle = wintypes.HANDLE()
    status_block = _IoStatusBlock()
    options = _FILE_OPEN_REPARSE_POINT | _FILE_SYNCHRONOUS_IO_NONALERT
    if directory:
        options |= _FILE_DIRECTORY_FILE
    access = _FILE_READ_ATTRIBUTES if metadata else _GENERIC_READ
    status = _nt_create_file(
        ctypes.byref(handle),
        access | _SYNCHRONIZE,
        ctypes.byref(attributes),
        ctypes.byref(status_block),
        None,
        0,
        _SHARE_ALL,
        _FILE_OPEN,
        options,
        None,
        0,
    )
    if status < 0:
        raise ctypes.WinError(_dos_error(status))
    return _descriptor(handle.value)


def open_directory(path: str | Path, *, dir_fd: int | None = None) -> int:
    """Open an approved root or child directory without following reparses."""
    if dir_fd is not None:
        return _open_child(str(path), dir_fd, directory=True, metadata=False)
    # The caller has canonicalized and approved this root. The extended
    # spelling also permits roots beyond the legacy MAX_PATH limit.
    spelling = str(path)
    if not spelling.startswith("\\\\?\\"):
        spelling = (
            "\\\\?\\UNC\\" + spelling[2:]
            if spelling.startswith("\\\\")
            else "\\\\?\\" + spelling
        )
    handle = _create_file(
        spelling,
        _GENERIC_READ,
        _SHARE_ALL,
        None,
        _OPEN_EXISTING,
        _FILE_FLAG_BACKUP_SEMANTICS | _FILE_FLAG_OPEN_REPARSE_POINT,
        None,
    )
    if handle == _INVALID_HANDLE_VALUE:
        raise ctypes.WinError(ctypes.get_last_error())
    return _descriptor(handle)


def open_file(path: str, *, dir_fd: int) -> int:
    """Open a child file for binary reading without following reparse points."""
    return _open_child(path, dir_fd, directory=False, metadata=False)


def stat_child(path: str, *, dir_fd: int) -> os.stat_result:
    """Inspect a child entry through a metadata handle without following it."""
    descriptor = _open_child(path, dir_fd, directory=False, metadata=True)
    try:
        return os.fstat(descriptor)
    finally:
        os.close(descriptor)
