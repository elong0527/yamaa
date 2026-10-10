Use only opensas v0.6.6 for this task.

- Write an opensas program to `/app/output/result.sas` that reads the input
  datasets from `/app/input` and writes every requested dataset.
- The program must run with `opensas /app/output/result.sas` to reproduce
  the output from a clean state. The executable is already installed.
- Use DATA steps, opensas expressions and supported procedures such as
  IMPORT, EXPORT, SORT and SQL. Do not install a runtime or packages.
- Do not use Python, R, Lua, shell execution from the program, language
  bridges or other languages to read, derive or write the data.
- Save each completed dataset as named in the task under `/app/output/`.

The language restriction applies to every tool call throughout the task,
including inspection, debugging, testing, runtime discovery and editing the
program. Calling another language or its package manager disqualifies the
trial even when the command fails or the final program is correct.

Use OpenCode's file read/write/edit tools, or shell text tools such as `ls`,
`cat`, `head`, `sed` and `grep`, to inspect inputs and edit source files.
Use the required language to calculate data and validate dataset values.
Do not probe or install other runtimes, use another language to generate the
program, or use Node/Bun merely because OpenCode uses them internally.

opensas is an already installed native executable, not a Python package.
Use `opensas --version` to check its version and
`opensas /app/output/result.sas` to execute the program. No pip, Python
imports, or runtime installation is needed. Confirm that execution produced
the requested files; exit zero alone is not evidence of successful output.
