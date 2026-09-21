# engine.R -- top-level orchestration for one spec.
#
# run_spec(spec_path, out_path): load inputs, build rows, derive columns,
# verify, publish. Returns the output path. Raises yamaa_error on failure.

run_spec <- function(spec_path, out_path, workflow_stack = character(0)) {
  spec <- resolve_spec(spec_path)
  spec_dir <- dirname(spec_path)
  validate_column_types(spec)
  validate_output_block(spec, out_path)
  # Stage 2: project functions resolve through environment.yaml
  # (src/functions.R); the loaded environment rides on the context.
  # A spec needing `function:` with a non-R project fails here, before
  # any specification data is read (REQ-0667).
  project_env <- check_function_runtime(spec, spec_dir)
  # project code: the harness treats the spec directory as the project
  # root and sources environment.R when present -- the local stand-in
  # for the artifact the environment pins
  project_fns <- new.env(parent = baseenv())
  env_r <- file.path(spec_dir, "environment.R")
  if (file.exists(env_r)) sys.source(env_r, envir = project_fns)
  inputs <- load_inputs(spec, spec_dir, spec_path, workflow_stack)
  # REQ-0104: a dataset identifier equal to the output domain fails
  if (!is.null(spec$domain) && spec$domain %in% names(inputs))
    yamaa_error("duplicate_identifier",
      paste0("input identifier equals output domain: ", spec$domain))
  keys <- spec$keys
  if (is.null(keys)) keys <- character(0)
  ctx0 <- list(
    inputs = inputs,
    keys = keys,
    n = 0L,
    col = list(),
    coltypes = character(0),
    key_indexes = list(),
    from_window = character(0),
    project_fns = project_fns,
    project_env = project_env,
    phase = "init",
    warn_env = { e <- new.env(parent = emptyenv()); e$entries <- list(); e },
    check_env = { e <- new.env(parent = emptyenv()); e$rows <- list(); e }
  )
  ctx0 <- init_intermediates(spec, ctx0)
  # REQ-0074: in a spec without rows, a key column derivation depending on
  # a non-key output column fails here, before row construction (keys are
  # derived before any row logic runs)
  if (is.null(spec$rows)) {
    colnames <- vapply(spec$columns, function(c) c$name, character(1))
    keys <- spec$keys
    if (is.null(keys)) keys <- character(0)
    for (c in spec$columns) {
      nm <- c$name
      if (nm %in% keys) {
        for (d in deriv_refs(c$derivation, colnames)) {
          if (!d %in% keys)
            yamaa_error("key_dependency",
              paste0("key column ", nm, " depends on non-key column ", d))
        }
      }
    }
  }
  # a failed run publishes the verification report alone (REQ-1181); a
  # successful run publishes report, violation log, then primary
  perr <- tryCatch({
    ctx <- build_rows(spec, ctx0)
    ctx <- derive_columns(spec, ctx)
    run_dataset_verifications(spec, ctx)
    publish_run(spec, ctx, out_path, failed = FALSE)
    NULL
  }, yamaa_error = function(e) e)
  if (!is.null(perr)) {
    publish_run(spec, ctx0, out_path, failed = TRUE)
    stop(perr)
  }
}

# REQ-0760/0391/1179/1180: validate the output block before any data is
# read: output.path and every declared sidecar name a mapped artifact
# profile, no two artifacts share a target, and a warning-severity run
# without a violation log fails up front (missing_violation_log).
validate_output_block <- function(spec, out_path) {
  check_profile <- function(p, what) {
    if (!is.null(p) && is.null(profile_of_path(p)))
      yamaa_error("unknown_artifact_profile",
        paste0(what, " has no mapped artifact profile: ", p))
  }
  check_profile(spec$output$path, "output.path")
  vlog <- spec$output$violation_log
  vrep <- spec$output$verification_report
  check_profile(vlog, "output.violation_log")
  check_profile(vrep, "output.verification_report")
  targets <- c(normalizePath(out_path, mustWork = FALSE),
    if (!is.null(vlog) && nzchar(vlog))
      normalizePath(file.path(dirname(out_path), vlog), mustWork = FALSE),
    if (!is.null(vrep) && nzchar(vrep))
      normalizePath(file.path(dirname(out_path), vrep), mustWork = FALSE))
  if (anyDuplicated(targets))
    yamaa_error("artifact_path_collision", "two published artifacts share a target path")
  if (spec_has_warning(spec) && (is.null(vlog) || !nzchar(vlog)))
    yamaa_error("missing_violation_log",
      "specification declares warning verifications but no output.violation_log (REQ-0391)")
  # REQ-0742/0743: output.decimals is meaningless for a parquet primary
  # artifact (parquet floats are written as the binary64 the derivation
  # produced)
  if (!is.null(spec$output$decimals) &&
      identical(profile_of_path(spec$output$path), "parquet"))
    yamaa_error("decimals_not_applicable",
      "output.decimals is meaningless for a parquet primary artifact")
}

# load every declared input dataset (undeclared columns default to str)
load_inputs <- function(spec, spec_dir, spec_path, workflow_stack = character(0)) {
  input_spec <- spec$input
  out <- list()
  # auto-discover from input/ directory if no spec
  if (is.null(input_spec)) {
    input_dir <- file.path(spec_dir, "input")
    if (dir.exists(input_dir)) {
      files <- list.files(input_dir, pattern = "\\.(csv|parquet)$", full.names = FALSE)
      for (f in files) {
        id <- toupper(sub("\\.(csv|parquet)$", "", f))
        path <- validate_input_path(file.path("input", f), spec_dir)
        out[[id]] <- read_input_profile(path, list(), list(), id, spec_dir, spec_path, workflow_stack)
      }
    }
    return(out)
  }
  for (id in names(input_spec)) {
    idef <- input_spec[[id]]
    if (is.character(idef)) idef <- list(path = idef)
    # REQ-0521: run producer before path validation (artifact may not exist)
    if (!is.null(idef$schema))
      ensure_producer_completed(id, idef, file.path(spec_dir, idef$path),
        spec_dir, spec_path, workflow_stack)
    path <- validate_input_path(idef$path, spec_dir)
    types <- idef$types
    if (is.null(types)) types <- list()
    # REQ-0523: the producing specification remains the only type authority;
    # `types` must be absent whenever `schema` is present, even for an entry
    # that agrees with the producer
    if (!is.null(idef$schema) && length(types) > 0) {
      fld <- names(types)[1]
      yamaa_error("redundant_field_type",
        paste0("types must be absent when schema is present: input.", id,
          ".types.", fld))
    }
    out[[id]] <- read_input_profile(path, idef, types, id, spec_dir, spec_path,
      workflow_stack)
  }
  out
}

# REQ-0516/0532/0533/0751/1161: select the reader by the input path's
# profile, apply the profile's empty-string convention, and validate a
# producing specification's contract when schema is declared.
read_input_profile <- function(path, idef, types, id, spec_dir, spec_path,
    workflow_stack = character(0)) {
  prof <- profile_of_path(path)
  if (is.null(prof))
    yamaa_error("source_profile_unknown",
      paste0("input ", id, ": no mapped source profile: ", path))
  empty_string <- "missing"
  if (!is.null(idef$empty_string)) {
    if (prof != "csv")
      yamaa_error("invalid_spec",
        paste0("input ", id, ": empty_string is a delimited-profile option: ", path))
    # REQ-1161: a delimited source must not declare the present convention
    if (idef$empty_string != "missing")
      yamaa_error("invalid_spec",
        paste0("input ", id, ": empty_string must not be \"present\": ", path))
    empty_string <- idef$empty_string
  }
  df <- if (prof == "csv") read_ydf_csv(path, types, empty_string)
       else read_ydf_parquet(path, types, empty_string)
  if (!is.null(idef$schema))
    validate_producer_contract(id, idef, df, spec_dir, spec_path, workflow_stack)
  df
}

# REQ-0521: the producer completes before the consumer reads the artifact.
# If the input artifact is missing, execute the producing specification
# (which validates recursively via run_spec); the stack guards cycles.
ensure_producer_completed <- function(id, idef, path, spec_dir, spec_path,
    workflow_stack = character(0)) {
  if (file.exists(path)) return(invisible(NULL))
  schema_path <- validate_input_path(idef$schema, spec_dir)
  norm_schema <- normalizePath(schema_path, mustWork = TRUE)
  if (norm_schema == normalizePath(spec_path, mustWork = TRUE) ||
      norm_schema %in% normalizePath(workflow_stack, mustWork = FALSE))
    yamaa_error("producer_contract_mismatch", paste0("input ", id,
      ": schema names consumer or workflow ancestor: ", idef$schema))
  producer <- tryCatch(yaml::read_yaml(schema_path),
    error = function(e) yamaa_error("invalid_spec",
      paste0("input ", id, ": cannot read producing spec: ", idef$schema)))
  p_out <- producer$output$path
  if (is.null(p_out) || !nzchar(p_out))
    yamaa_error("invalid_spec", paste0("input ", id,
      ": producing spec has no output.path: ", idef$schema))
  run_spec(schema_path, file.path(dirname(schema_path), p_out),
    c(workflow_stack, spec_path))
}

# REQ-0521/0522/0534/0535: when schema names a producing specification, the
# stored artifact's field names, order, and self-describing types must
# exactly match the producer's output contract (its output.columns, in
# artifact order, with each field's declared type). A self-link fails.
# REQ-0521: the dependency graph is acyclic -- the stack detects a link
# naming the consumer or one already above it in the workflow.
validate_producer_contract <- function(id, idef, df, spec_dir, spec_path,
    workflow_stack = character(0)) {
  schema_path <- validate_input_path(idef$schema, spec_dir)
  norm_schema <- normalizePath(schema_path, mustWork = TRUE)
  norm_spec <- normalizePath(spec_path, mustWork = TRUE)
  # REQ-0521: a link cannot name the consuming specification or one already
  # above it in the workflow (cycle)
  if (norm_schema == norm_spec ||
      norm_schema %in% normalizePath(workflow_stack, mustWork = FALSE))
    yamaa_error("producer_contract_mismatch", paste0("input ", id,
      ": schema names the consuming specification or a workflow ancestor: ",
      idef$schema))
  producer <- tryCatch(yaml::read_yaml(schema_path),
    error = function(e) yamaa_error("invalid_spec",
      paste0("input ", id, ": cannot read producing spec: ", idef$schema)))
  pcols <- vapply(producer$columns, function(c) c$name, character(1))
  ptypes <- vapply(producer$columns, function(c) c$type, character(1))
  names(ptypes) <- pcols
  # REQ-0534: the document must declare typed columns to be a specification
  if (length(pcols) == 0 || any(!nzchar(pcols)) || any(is.na(ptypes)))
    yamaa_error("invalid_spec", paste0("input ", id,
      ": not a complete yamaa specification: ", idef$schema))
  # REQ-0522: output.columns names every stored field exactly once, in
  # artifact order; internal columns omitted from it are not stored
  ocols <- producer$output$columns
  if (is.null(ocols)) ocols <- pcols
  got <- names(df)
  gottypes <- attr(df, "coltypes")
  if (!identical(unname(got), unname(ocols)))
    yamaa_error("producer_contract_mismatch", paste0("input ", id,
      ": fields do not match producing spec ", idef$schema,
      ": got [", paste(got, collapse = ", "), "]"))
  bad <- ocols[is.na(ptypes[ocols]) | gottypes[ocols] != ptypes[ocols]]
  if (length(bad) > 0)
    yamaa_error("producer_contract_mismatch", paste0("input ", id,
      ": field types do not match producing spec ", idef$schema,
      ": ", paste(bad, collapse = ", ")))
  invisible(NULL)
}

# validate an input path per REQ-0775..0785: must be relative, no URI scheme,
# must not escape the project, must exist and be a regular file (not a
# directory; symlinks rejected).
validate_input_path <- function(p, spec_dir) {
  if (grepl("^[a-zA-Z][a-zA-Z0-9+.-]*://", p))
    yamaa_error("resource_path_uri_scheme", paste0("URI scheme not allowed: ", p))
  if (grepl("^/", p) || grepl("^[a-zA-Z]:", p))
    yamaa_error("resource_path_not_relative", paste0("path must be relative: ", p))
  # normalize .. segments textually; escape fails
  segs <- strsplit(p, "/", fixed = TRUE)[[1]]
  depth <- 0L
  for (s in segs) {
    if (s %in% c("", ".")) next
    if (s == "..") depth <- depth - 1L else depth <- depth + 1L
    if (depth < 0)
      yamaa_error("resource_path_outside_project", paste0("path escapes project: ", p))
  }
  full <- file.path(spec_dir, p)
  if (!file.exists(full) && !dir.exists(full))
    yamaa_error("resource_path_missing", paste0("path does not exist: ", p))
  if (dir.exists(full))
    yamaa_error("resource_path_not_regular_file", paste0("not a regular file: ", p))
  if (Sys.readlink(full) != "")
    yamaa_error("resource_path_symlink", paste0("symlink not allowed: ", p))
  full
}
