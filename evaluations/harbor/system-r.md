Use only R for this task.

- Write an R script to `/app/output/result.R` that reads the input
  datasets from `/app/input` and writes every requested dataset.
- The script must run with `Rscript /app/output/result.R` to
  reproduce the output from a clean state.
- Use only the R packages in the sandbox (dplyr, tidyr, readr,
  stringr, lubridate, purrr, haven, xml2, nanoparquet). Do not
  install packages.
- Do not use Python, reticulate, or system calls to other languages.
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
