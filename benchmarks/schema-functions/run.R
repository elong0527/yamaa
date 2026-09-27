library(yamaa)

test <- run_with_project_functions("spec.yaml", project_root = "r")$output
test
