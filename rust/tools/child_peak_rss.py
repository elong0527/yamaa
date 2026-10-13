"""Observe the launched child's lifetime high-water RSS, never the controller's.

Unix wait4 returns the reaped child's original kernel observation. Windows
GetProcessMemoryInfo reads PeakWorkingSetSize while Popen retains the process
handle, including after exit. Neither provider samples RSS or estimates a peak.
"""

from __future__ import annotations

import os
import platform

SCOPE = "whole-child-lifetime; cumulative across warm repetitions; includes collector transport"


def wait(process):
    system = platform.system()
    if system in {"Linux", "Darwin"} and hasattr(os, "wait4"):
        pid, status, usage = os.wait4(process.pid, 0)
        if pid != process.pid:
            raise RuntimeError("wait4 returned another process")
        process.returncode = os.waitstatus_to_exitcode(status)
        raw = usage.ru_maxrss
        observation = {
            "provider": "os.wait4(child_pid).ru_maxrss",
            "raw": raw,
            "original_unit": "KiB" if system == "Linux" else "bytes",
            "bytes": raw * (1024 if system == "Linux" else 1),
            "scope": SCOPE,
            "system": system,
        }
    elif system == "Windows":
        process.wait()
        observation = windows(process)
    else:
        process.wait()
        observation = None
    return process.returncode, observation


def windows(process):
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            *[(name, ctypes.c_size_t) for name in (
                "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
            )],
        ]

    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    query = ctypes.WinDLL("psapi", use_last_error=True).GetProcessMemoryInfo
    query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    query.restype = wintypes.BOOL
    if not query(wintypes.HANDLE(int(process._handle)), ctypes.byref(counters), counters.cb):
        raise ctypes.WinError(ctypes.get_last_error())
    raw = counters.PeakWorkingSetSize
    return {
        "provider": "GetProcessMemoryInfo(child_handle).PeakWorkingSetSize",
        "raw": raw, "original_unit": "bytes", "bytes": raw,
        "scope": SCOPE, "system": "Windows",
    }


def validate(observation, system):
    if system not in {"Linux", "Darwin", "Windows"}:
        if observation is not None:
            raise ValueError("unexpected RSS for an unadmitted provider")
        return
    if not isinstance(observation, dict):
        raise ValueError("child peak RSS provider unavailable")
    provider = ("GetProcessMemoryInfo(child_handle).PeakWorkingSetSize" if system == "Windows"
                else "os.wait4(child_pid).ru_maxrss")
    raw = observation.get("raw")
    if (isinstance(raw, bool) or not isinstance(raw, (int, float)) or raw <= 0
            or observation.get("provider") != provider
            or observation.get("system") != system
            or observation.get("original_unit") != ("KiB" if system == "Linux" else "bytes")
            or observation.get("bytes") != raw * (1024 if system == "Linux" else 1)
            or observation.get("scope") != SCOPE):
        raise ValueError("invalid child peak RSS observation")


def main():
    # Launch from this small, newly exec'd broker. A large controller's fork RSS
    # otherwise survives exec as the child's ru_maxrss, contaminating even the
    # worker's own getrusage observation with workload-generation memory.
    import argparse
    import json
    import subprocess
    import signal
    from pathlib import Path

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--command", required=True)
    parser.add_argument("--record", type=Path, required=True)
    args = parser.parse_args()
    process = subprocess.Popen(json.loads(args.command))
    args.record.write_text(json.dumps({"state": "running", "pid": process.pid,
                                      "peak_rss": None}) + "\n", encoding="utf-8")
    if os.name != "nt":
        def interrupted(signum, frame):
            raise KeyboardInterrupt("RSS broker interrupted")
        signal.signal(signal.SIGTERM, interrupted)
    try:
        code, peak = wait(process)
        record = {"exit_code": code, "pid": process.pid, "peak_rss": peak,
                  "launcher": "fresh small broker; controller fork RSS excluded"}
    except BaseException:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        raise
    args.record.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
