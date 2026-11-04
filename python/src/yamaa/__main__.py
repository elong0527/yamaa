"""`python -m yamaa`: the command line run as the Python host.

It takes the arguments the `yamaa` binary takes, runs them over this
interpreter's engine with the project functions installed here, prints what
the binary prints on this process's standard streams, and exits with the
binary's exit status. Ctrl+C raises `KeyboardInterrupt`, as every call of the
package does.
"""

import sys

import yamaa

if __name__ == "__main__":
    sys.exit(yamaa.cli(sys.argv[1:]))
