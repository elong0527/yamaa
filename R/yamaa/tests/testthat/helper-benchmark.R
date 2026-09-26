# helper-benchmark.R -- compact port of the clean-room tree's run-benchmark.R
# for the packaged engine: run every manifest entry through yamaa::run_spec
# and compare against expected/ artifacts. The tree's runner is the
# reference; this helper mirrors its semantics (condition-only comparison
# for negatives, text CSV comparison for positives).

yamaa_handlers <- function() {
  list("bool#yes" = function(x) if (x %in% c("Y", "y")) x else TRUE,
       "bool#no" = function(x) if (x %in% c("N", "n")) x else FALSE)
}

yamaa_benchmarks_root <- function() {
  env <- Sys.getenv("YAMAA_BENCHMARKS", "")
  if (nzchar(env)) return(env)
  # tests run from R/yamaa/tests: benchmarks live at the repo root
  file.path(testthat::test_path(), "..", "..", "..", "..", "benchmarks")
}

yamaa_manifest <- function(bdir) {
  yaml::yaml.load_file(file.path(bdir, "execution-manifest.yaml"),
    handlers = yamaa_handlers())$examples
}

# plain CSV reader for golden comparison (text compare, no typing)
read_yamaa_csv <- function(path) {
  raw <- yamaa:::read_raw_bytes(path)
  text <- rawToChar(raw)
  recs <- yamaa:::split_csv_records(text, path)
  if (length(recs) == 0) return(data.frame())
  header <- recs[[1]]
  body <- recs[-1]
  if (length(body) > 0 && all(body[[length(body)]] == "")) body <- body[-length(body)]
  df <- as.data.frame(do.call(rbind, lapply(body, function(f) {
    if (length(f) < length(header))
      f <- c(f, rep("", length(header) - length(f)))
    f
  })), stringsAsFactors = FALSE, check.names = FALSE)
  if (ncol(df) == 0 && length(header) > 0) {
    df <- as.data.frame(matrix(nrow = 0, ncol = length(header)),
      stringsAsFactors = FALSE)
  }
  names(df) <- header
  df[df == ""] <- NA_character_
  df
}

compare_csv <- function(got_path, exp_path) {
  got <- read_yamaa_csv(got_path)
  exp <- read_yamaa_csv(exp_path)
  gc <- names(got); ec <- names(exp)
  if (!identical(gc, ec))
    return(paste0("columns differ: got [", paste(gc, collapse = ","),
      "] expected [", paste(ec, collapse = ","), "]"))
  if (nrow(got) != nrow(exp))
    return(paste0("row count ", nrow(got), " != expected ", nrow(exp)))
  for (j in seq_along(gc)) {
    gv <- ifelse(is.na(got[[j]]), "", got[[j]])
    ev <- ifelse(is.na(exp[[j]]), "", exp[[j]])
    bad <- which(gv != ev)
    if (length(bad) > 0)
      return(paste0("column ", gc[j], ": ", length(bad),
        " cells differ; first row ", bad[1],
        " got [", gv[bad[1]], "] expected [", ev[bad[1]], "]"))
  }
  TRUE
}

# structural comparison of two parsed YAML values (resolved-spec check)
compare_yaml <- function(got, exp, trail = "$") {
  # strip internal provenance attributes (yamaa_orig) before comparison
  strip_attr <- function(x) {
    if (is.list(x)) return(lapply(x, strip_attr))
    attr(x, "yamaa_orig") <- NULL
    x
  }
  got <- strip_attr(got)
  if (is.list(got) && is.list(exp)) {
    gn <- names(got); en <- names(exp)
    if (is.null(gn) != is.null(en))
      return(paste0(trail, ": named vs unnamed list"))
    if (!is.null(gn) && !identical(gn, en))
      return(paste0(trail, ": keys differ: got [", paste(gn, collapse = ","),
        "] expected [", paste(en, collapse = ","), "]"))
    if (length(got) != length(exp))
      return(paste0(trail, ": length ", length(got), " != ", length(exp)))
    for (i in seq_along(got)) {
      nm <- if (!is.null(gn)) paste0(trail, ".", gn[i])
        else paste0(trail, "[", i, "]")
      r <- compare_yaml(got[[i]], exp[[i]], nm)
      if (!isTRUE(r)) return(r)
    }
    return(TRUE)
  }
  if (is.numeric(got) && is.numeric(exp)) {
    if (length(got) != length(exp) || any(is.na(got) != is.na(exp)) ||
        any(got != exp, na.rm = TRUE))
      return(paste0(trail, ": got ", deparse1(got),
        " expected ", deparse1(exp)))
    return(TRUE)
  }
  if (!identical(got, exp))
    return(paste0(trail, ": got ", deparse1(got),
      " expected ", deparse1(exp)))
  TRUE
}

# structural parquet comparison: identical embedded schema (names,
# order, physical/logical types) and identical typed values
compare_parquet <- function(got_path, exp_path) {
  type_strs <- function(p) {
    tab <- arrow::read_parquet(p)
    vapply(tab$schema$fields, function(f) f$type$ToString(), character(1))
  }
  name_strs <- function(p) {
    tab <- arrow::read_parquet(p)
    vapply(tab$schema$fields, function(f) f$name, character(1))
  }
  gn <- name_strs(got_path); en <- name_strs(exp_path)
  if (!identical(gn, en))
    return(paste0("columns differ: got [", paste(gn, collapse = ","),
      "] expected [", paste(en, collapse = ","), "]"))
  gs <- type_strs(got_path); es <- type_strs(exp_path)
  if (!identical(gs, es))
    return(paste0("parquet types differ: got [", paste(gs, collapse = ","),
      "] expected [", paste(es, collapse = ","), "]"))
  gdf <- yamaa:::read_ydf_parquet(got_path)
  edf <- yamaa:::read_ydf_parquet(exp_path)
  if (nrow(gdf) != nrow(edf))
    return(paste0("row count ", nrow(gdf), " != expected ", nrow(edf)))
  for (j in seq_along(gn)) {
    if (!identical(gdf[[j]], edf[[j]]))
      return(paste0("column ", gn[j], ": values differ"))
  }
  TRUE
}

# dispatch the artifact comparison on the expected file's profile
compare_artifact <- function(got_path, exp_path) {
  if (identical(yamaa:::profile_of_path(exp_path), "parquet"))
    compare_parquet(got_path, exp_path)
  else compare_csv(got_path, exp_path)
}

# run one benchmark entry; returns list(status, detail)
run_one_benchmark <- function(nm, wt, man_entry) {
  bdir <- file.path(wt, "benchmarks", nm)
  spec_path <- file.path(bdir, "spec.yaml")
  if (!file.exists(spec_path)) spec_path <- file.path(bdir, "spec_study.yaml")
  if (!file.exists(spec_path))
    return(list(status = "SKIP", detail = "no spec.yaml"))
  is_neg <- startsWith(nm, "negative-")
  # resolve the spec first so the output profile selects the temp extension
  spec0 <- tryCatch(yamaa:::resolve_spec(spec_path), error = function(e) NULL)
  ext <- ".csv"
  if (!is.null(spec0) && !is.null(spec0$output$path)) {
    p0 <- yamaa:::profile_of_path(spec0$output$path)
    if (!is.null(p0)) ext <- paste0(".", p0)
  }
  out_path <- tempfile(fileext = ext)
  err <- tryCatch({
    yamaa::run_spec(spec_path, out_path)
    NULL
  }, yamaa_error = function(e) paste0(attr(e, "yamaa_condition"), ": ",
      conditionMessage(e)),
    error = function(e) paste0("R error: ", conditionMessage(e)))
  if (is_neg) {
    exp_err <- tryCatch(
      yaml::yaml.load_file(file.path(bdir, "expected", "error.yaml"),
        handlers = yamaa_handlers()),
      error = function(e) NULL)
    if (is.null(err))
      return(list(status = "FAIL", detail = "negative benchmark succeeded"))
    if (is.null(exp_err) || is.null(exp_err$condition))
      return(list(status = "FAIL", detail = "no expected condition"))
    got_cond <- sub(":.*$", "", err)
    if (identical(got_cond, exp_err$condition))
      return(list(status = "PASS",
        detail = paste0("failed as expected: ", substr(err, 1, 120))))
    return(list(status = "FAIL",
      detail = paste0("wrong condition: got [", got_cond,
        "] expected [", exp_err$condition, "]")))
  }
  if (!is.null(err) && startsWith(err, "runner_language_mismatch:"))
    return(list(status = "SKIP", detail = err))
  if (!is.null(err))
    return(list(status = "FAIL", detail = err))
  spec <- spec0
  if (is.null(spec)) spec <- yamaa:::resolve_spec(spec_path)
  exp_dir <- file.path(bdir, "expected")
  exp_primary <- file.path(exp_dir, basename(spec$output$path))
  if (!file.exists(exp_primary)) {
    pat <- if (identical(ext, ".parquet")) "\\.parquet$" else "\\.csv$"
    exp_files <- list.files(exp_dir, pattern = pat, full.names = TRUE)
    if (length(exp_files) == 0)
      return(list(status = "FAIL", detail = "no expected artifact"))
    exp_primary <- exp_files[1]
  }
  cmp <- compare_artifact(out_path, exp_primary)
  if (!isTRUE(cmp)) return(list(status = "FAIL", detail = cmp))
  vlog <- spec$output$warning_log
  if (!is.null(vlog) && nzchar(vlog)) {
    got_vlog <- file.path(dirname(out_path), vlog)
    exp_vlog <- file.path(exp_dir, vlog)
    if (!file.exists(got_vlog))
      return(list(status = "FAIL",
        detail = paste0("violation log not produced: ", vlog)))
    if (!file.exists(exp_vlog))
      return(list(status = "FAIL",
        detail = paste0("no expected violation log: ", vlog)))
    cmp2 <- compare_artifact(got_vlog, exp_vlog)
    if (!isTRUE(cmp2))
      return(list(status = "FAIL", detail = paste0("violation log: ", cmp2)))
  }
  vrep <- spec$output$verification_log
  if (!is.null(vrep) && nzchar(vrep)) {
    got_vrep <- file.path(dirname(out_path), vrep)
    exp_vrep <- file.path(exp_dir, vrep)
    if (!file.exists(got_vrep))
      return(list(status = "FAIL",
        detail = paste0("verification report not produced: ", vrep)))
    if (!file.exists(exp_vrep))
      return(list(status = "FAIL",
        detail = paste0("no expected verification report: ", vrep)))
    cmp2b <- compare_artifact(got_vrep, exp_vrep)
    if (!isTRUE(cmp2b))
      return(list(status = "FAIL",
        detail = paste0("verification report: ", cmp2b)))
  }
  exp_resolved <- file.path(exp_dir, "spec_resolved.yaml")
  if (file.exists(exp_resolved)) {
    exp_spec <- yaml::yaml.load_file(exp_resolved, handlers = yamaa_handlers())
    cmp3 <- compare_yaml(spec, exp_spec)
    if (!isTRUE(cmp3))
      return(list(status = "FAIL",
        detail = paste0("spec_resolved mismatch: ", cmp3)))
  }
  list(status = "PASS", detail = "")
}

# Tolerated baseline: pre-existing clean-room parity gaps, identical in the
# unpackaged tree (FINDINGS.md, daily parity entries, 287/293). When a gap
# closes, delete its name here so the suite keeps shrinking toward zero.
benchmark_known_failures <- c(
  "adam-advs-windows",
  "negative-output-missing-key",
  "negative-str-contains-bool-result",
  "sdtm-tr-tumor-measurements",
  "sdtm-vs-collected-form"
)
benchmark_known_skips <- c(
  "sdtm-dm-race-ethnicity"  # no spec.yaml in the corpus
)

# Classify benchmark fail/skip entries against the tolerated baseline.
# fails/skips are "name: detail" strings. The baseline ships as default
# arguments so unit tests can inject synthetic baselines. Returns a list:
#   new_fails, new_skips       -- entries outside the baseline (must be empty)
#   recovered_fails, recovered_skips -- baseline names absent from this run
classify_benchmark_outcomes <- function(fails, skips,
                                        known_failures = benchmark_known_failures,
                                        known_skips = benchmark_known_skips) {
  fail_names <- sub(":.*$", "", fails)
  skip_names <- sub(":.*$", "", skips)
  # the only acceptable skips are the Stage-2 function-runtime benchmarks
  # (runner_language_mismatch) plus the known-skip baseline
  acceptable <- grepl("runner_language_mismatch", skips, fixed = TRUE)
  list(
    new_fails = fails[!fail_names %in% known_failures],
    new_skips = skips[!acceptable & !skip_names %in% known_skips],
    recovered_fails = setdiff(known_failures, fail_names),
    recovered_skips = setdiff(known_skips, skip_names)
  )
}
