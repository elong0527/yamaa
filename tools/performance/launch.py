"""Run one command and record its wall time, exit status and peak RSS.

Usage: python -I -S launch.py RESULT COMMAND...; writes `seconds max_rss
status` to RESULT. Linux carries a process's peak RSS across exec, so a
command started straight from the runner would report at least the runner's
own peak, which grows with the frames it compares. This launcher starts
small, forks and execs the command, and reads the child's own peak from wait4.
"""

import os
import sys
import time


def main():
    result, command = sys.argv[1], sys.argv[2:]
    start = time.perf_counter()
    pid = os.fork()
    if pid == 0:
        try:
            os.execvp(command[0], command)
        finally:
            os._exit(127)
    _, status, usage = os.wait4(pid, 0)
    seconds = time.perf_counter() - start
    with open(result, "w") as out:
        out.write(f"{seconds} {usage.ru_maxrss} {os.waitstatus_to_exitcode(status)}\n")


if __name__ == "__main__":
    main()
