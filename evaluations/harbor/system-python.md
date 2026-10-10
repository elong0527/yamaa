Use only Python for this task.

- Write a Python script to `/app/output/result.py` that reads the
  input datasets from `/app/input` and writes every requested dataset.
- The script must run with `python3 /app/output/result.py` to
  reproduce the output from a clean state.
- Use only the Python packages in the sandbox (numpy, pandas, polars,
  pyarrow, lxml). Do not install packages.
- Do not use R, subprocess calls to Rscript, or other languages.
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
