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
