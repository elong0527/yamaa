# Installed R worker for public-lifecycle/1. Run as an Rscript file with argv.
# jsonlite transports JSON; microbenchmark::get_nanotime supplies an explicitly
# admitted raw clock. No microbenchmark expression wrapper/overhead subtraction.
# No candidate renderer or expected report is used to construct expected truth.

args <- commandArgs(trailingOnly = TRUE)
stopifnot(length(args) == 1L)
# Collector bootstrap precedes the yamaa import boundary and is retained. Paths
# are decoded from JSON, never interpolated into an R expression by the launcher.
bootstrap <- readLines(args[[1L]], warn = FALSE, encoding = "UTF-8")
# The controller supplies R_LIBS via the launcher environment. jsonlite must be
# available there to decode the request; it is an explicit collector dependency.
if (!requireNamespace("jsonlite", quietly = TRUE)) stop("collector jsonlite is absent")
request <- jsonlite::fromJSON(paste(bootstrap, collapse = "\n"), simplifyVector = FALSE)
collector_paths <- vapply(request$provenance$collectors, function(x) dirname(x$origin), "")
.libPaths(unique(c(dirname(request$provenance$installed_origins$package), collector_paths, .libPaths())))
if (!requireNamespace("microbenchmark", quietly = TRUE)) stop("collector microbenchmark is absent")
output <- request$output
write_bytes <- function(path, bytes) {
  if (endsWith(path, ".gz")) {
    target <- gzfile(path, "wb", compression = 1L)
    on.exit(close(target))
    writeBin(bytes, target)
  } else writeBin(bytes, path)
}
retain_envelope <- function(raw, report_path, measurements_path) {
  prefix <- '{"protocol":"held-observations/1","report":'
  delimiter <- ',"measurements":'
  text <- rawToChar(raw)
  if (!startsWith(text, prefix) || !endsWith(text, "}")) stop("invalid held observation envelope")
  positions <- gregexpr(delimiter, text, fixed = TRUE, useBytes = TRUE)[[1L]]
  split <- tail(positions, 1L)
  if (split < nchar(prefix, type = "bytes")) stop("missing original measurement evidence")
  rm(text)
  # Retain exact raw JSON integers and report bytes; no report cell DOM and no
  # R binary64 conversion of uint64 measurement offsets/counts.
  ranges <- list(c(nchar(prefix, type = "bytes") + 1L, split - 1L),
                 c(split + nchar(delimiter, type = "bytes"), length(raw) - 1L))
  paths <- list(report_path, measurements_path)
  for (index in seq_along(paths)) {
    target <- if (endsWith(paths[[index]], ".gz")) gzfile(paths[[index]], "wb", compression = 1L) else file(paths[[index]], "wb")
    tryCatch({
      for (start in seq.int(ranges[[index]][[1L]], ranges[[index]][[2L]], by = 1048576L)) {
        end <- min(start + 1048575L, ranges[[index]][[2L]])
        writeBin(raw[start:end], target)
      }
    }, finally = close(target))
  }
}
write_json <- function(path, value) {
  json <- jsonlite::toJSON(value, auto_unbox = TRUE, null = "null", na = "null",
                          digits = NA, pretty = TRUE, force = TRUE)
  write_bytes(path, charToRaw(enc2utf8(paste0(json, "\n"))))
}
failure <- function(error) {
  list(class = as.list(class(error)), message = conditionMessage(error),
       call = if (is.null(conditionCall(error))) NULL else paste(deparse(conditionCall(error)), collapse = "\n"))
}
response <- list(protocol = request$protocol, child = request$child, pid = Sys.getpid(),
                 import_duration = NULL, repetitions = list(), failure = NULL,
                 collector_bootstrap = list(
                   scope = "collector import/request decode before yamaa import timing",
                   collectors = lapply(names(request$provenance$collectors), function(name) {
                     list(name = name, version = as.character(utils::packageVersion(name)),
                          origin = normalizePath(find.package(name), winslash = "/", mustWork = TRUE))
                   })))
write_json(file.path(output, "response.json"), response)
clock <- microbenchmark::get_nanotime
binary64_number <- function(value) {
  list(decimal = sprintf("%.17g", value),
       binary64_bits = paste(sprintf("%02x", as.integer(writeBin(as.double(value), raw(),
                                                               size = 8L, endian = "big"))),
                             collapse = ""))
}
timed <- function(state, name, expr) {
  start <- clock()
  on.exit({
    end <- clock()
    # JSON numeric printing can round a large double even with digits=NA.
    # Preserve the returned doubles themselves; never round or repair elapsed.
    state$durations[[name]] <- list(representation = "binary64-nanoseconds",
                                  start_ns = binary64_number(start),
                                  end_ns = binary64_number(end),
                                  duration_ns = binary64_number(end - start))
  })
  force(expr)
}
storage_kind <- function(column) {
  if (inherits(column, "yamaa_int64_vector")) return("int")
  if (inherits(column, "yamaa_utf8_vector")) return("str")
  if (inherits(column, "Date")) return("date")
  if (inherits(column, "POSIXct")) return("datetime")
  switch(typeof(column), character = "str", integer = "int", double = "float",
         stop("unsupported actual copied column type"))
}
cell <- function(column, index, kind) {
  if (inherits(column, "yamaa_int64_vector") || inherits(column, "yamaa_utf8_vector")) {
    value <- unclass(column)[[index]]
  } else {
    value <- column[index]
  }
  missing <- is.null(value) || (length(value) == 1L && is.na(value) &&
                              !(is.double(value) && is.nan(value)))
  if (missing) return(list(type = "missing", value = NULL))
  encoded <- switch(kind,
    float = paste(sprintf("%02x", as.integer(writeBin(as.double(value), raw(), size = 8L,
                                                      endian = "big"))), collapse = ""),
    int = as.character(value),
    date = format(value, "%Y-%m-%d"),
    datetime = format(value, "%Y-%m-%dT%H:%M:%S", tz = "UTC"),
    str = as.character(value), stop("unsupported cell kind"))
  list(type = kind, value = encoded)
}
frame <- function(value) {
  if (is.null(value)) return(NULL)
  stopifnot(is.data.frame(value))
  kinds <- vapply(value, storage_kind, "")
  list(columns = lapply(seq_along(value), function(i) list(names(value)[i], kinds[i])),
       row_count = nrow(value),
       rows = lapply(seq_len(nrow(value)), function(row) {
         lapply(seq_along(value), function(i) cell(value[[i]], row, kinds[i]))
       }),
       host_schema = lapply(seq_along(value), function(i) {
         list(name = names(value)[i], typeof = typeof(value[[i]]), class = as.list(class(value[[i]])))
       }))
}
issues <- function(value) {
  stopifnot(is.data.frame(value))
  kind <- function(x) {
    if (is.character(x)) "str" else if (is.list(x) && all(vapply(x, is.character, TRUE))) "list[str]" else stop("unexpected issue column type")
  }
  list(columns = lapply(seq_along(value), function(i) list(names(value)[i], kind(value[[i]]))),
       row_count = nrow(value),
       rows = lapply(seq_len(nrow(value)), function(row) {
         lapply(seq_along(value), function(index) {
           column <- value[[index]]
           if (is.list(column)) as.list(column[[row]]) else column[row]
         })
       }))
}
metadata <- function() {
  collectors <- lapply(names(request$provenance$collectors), function(name) {
    list(version = as.character(utils::packageVersion(name)),
         origin = normalizePath(find.package(name), winslash = "/", mustWork = TRUE))
  })
  names(collectors) <- names(request$provenance$collectors)
  list(host = "r", runtime_version = as.character(getRversion()),
       report_runtime_version = as.character(getRversion()),
       package_version = as.character(utils::packageVersion("yamaa")),
       engine_version = if (exists(".core_version", asNamespace("yamaa"), inherits = FALSE)) get(".core_version", asNamespace("yamaa"))() else NULL,
       engine_version_availability = "compiled metadata accessor when available; source build attestation retained",
       installed_origins = list(package = normalizePath(find.package("yamaa"), winslash = "/", mustWork = TRUE),
                                native = normalizePath(getLoadedDLLs()[["yamaa"]][["path"]], winslash = "/", mustWork = TRUE)),
       system = unname(Sys.info()[["sysname"]]), machine = unname(Sys.info()[["machine"]]),
       cpu_observation = NULL, library_paths = as.list(.libPaths()),
       collectors = collectors,
       clock_library = normalizePath(getLoadedDLLs()[["microbenchmark"]][["path"]], winslash = "/", mustWork = TRUE),
       clock = list(provider = request$provenance$clock_evidence$provider,
                    api = "microbenchmark::get_nanotime", unit = "ns",
                    representation = "lossless binary64 decimal strings and IEEE bytes; no overhead adjustment"))
}
import_state <- new.env(parent = emptyenv())
import_state$durations <- list()
ok <- tryCatch({
  if ("yamaa" %in% loadedNamespaces()) stop("candidate namespace already loaded before the import boundary")
  timed(import_state, "package_import", {
    library("yamaa", character.only = TRUE)
    domain <- yamaa::yamaa_domain
  })
  TRUE
}, error = function(error) { response$failure <<- failure(error); FALSE },
   interrupt = function(error) { response$failure <<- failure(error); FALSE })
response$import_duration <- import_state$durations$package_import
write_json(file.path(output, "response.json"), response)
if (!ok) quit(status = 1L)
for (declaration in request$repetitions) {
  state <- new.env(parent = emptyenv())
  state$durations <- list()
  row <- list(index = declaration$index, declaration = declaration, status = "failure",
              durations = list(), metadata = NULL, native_call_order = list(),
              unavailable = request$unavailable, rss = NULL, failure = NULL,
              reports = list(), saved_files = list(), save_returns = list(),
              measurements = list(), evidence_capture_failures = list(), host_view_access_counts = list(), callback_observation = NULL,
              views = paste0("rep-", declaration$index, "-views.json", if (isTRUE(request$compress_evidence)) ".gz" else ""))
  views <- list()
  result <- NULL
  prefix <- paste0("rep-", declaration$index)
  slot <- length(response$repetitions) + 1L
  persist <- function() {
    row$durations <<- state$durations
    response$repetitions[[slot]] <<- row
    write_json(file.path(output, row$views), views)
    write_json(file.path(output, "response.json"), response)
  }
  persist()
  interrupted <- FALSE
  target <- file.path(declaration$work_directory, declaration$output_name)
  secondary_conditions <- list()
  secondary_failure <- function(error, location) {
    secondary_conditions <<- c(secondary_conditions, list(error))
    row$evidence_capture_failures <<- c(row$evidence_capture_failures,
                                       list(c(list(location = location), failure(error))))
  }
  retain_reached <- function(label, include_output = FALSE) {
    # Safe finally transport: never retry a Build/Save or replace its public cause.
    tryCatch({
      filename <- paste0(prefix, "-", label, "-report.json", if (isTRUE(request$compress_evidence)) ".gz" else "")
      if (isTRUE(request$phase_measurements)) {
        raw <- get(".domain_observations", asNamespace("yamaa"))(result, include_measurements = TRUE)
        metrics <- paste0(prefix, "-", label, "-measurements.json", if (isTRUE(request$compress_evidence)) ".gz" else "")
        retain_envelope(raw, file.path(output, filename), file.path(output, metrics))
        row$measurements[[label]] <<- metrics
      } else {
        raw <- get(".domain_observations", asNamespace("yamaa"))(result)
        write_bytes(file.path(output, filename), if (is.null(raw)) charToRaw("null") else raw)
      }
      row$reports[[label]] <<- filename
    }, error = function(error) secondary_failure(error, paste0(label, "_report")),
       interrupt = function(error) secondary_failure(error, paste0(label, "_report")))
    if (include_output) {
      tryCatch({
        if (file.exists(target)) {
          filename <- paste0(prefix, "-", label, "-", declaration$output_name, if (isTRUE(request$compress_evidence)) ".gz" else "")
          size <- file.info(target)$size
          if (is.na(size)) stop("reached output size is unavailable")
          bytes <- readBin(target, "raw", n = size)
          write_bytes(file.path(output, filename), bytes)
          row$saved_files[[label]] <<- filename
        }
        for (name in declaration$artifact_names) {
          if (identical(name, declaration$output_name) && !is.null(row$saved_files[[label]])) {
            row$saved_artifacts[[label]][[name]] <<- row$saved_files[[label]]
            next
          }
          artifact <- file.path(declaration$work_directory, name)
          if (file.exists(artifact)) {
            filename <- paste0(prefix, "-", label, "-", name, if (isTRUE(request$compress_evidence)) ".gz" else "")
            bytes <- readBin(artifact, "raw", n = file.info(artifact)$size)
            write_bytes(file.path(output, filename), bytes)
            row$saved_artifacts[[label]][[name]] <<- filename
          }
        }
      }, error = function(error) secondary_failure(error, paste0(label, "_output")),
         interrupt = function(error) secondary_failure(error, paste0(label, "_output")))
    }
  }
  callback_namespace <- NULL
  original_callback <- NULL
  callback_trace <- list()
  if (isTRUE(request$observe_callbacks)) {
    callback_namespace <- asNamespace("yamaabenchmarks")
    original_callback <- get("project_value", callback_namespace)
    observed_callback <- function(x) {
      trace <- list(parameters = list(x = cell(x, 1L, "float")), result = NULL, failure = NULL)
      slot <- length(callback_trace) + 1L
      callback_trace[[slot]] <<- trace
      returned <- tryCatch(original_callback(x = x), error = function(error) {
        trace$failure <- failure(error)
        callback_trace[[slot]] <<- trace
        stop(error)
      }, interrupt = function(error) {
        trace$failure <- failure(error)
        callback_trace[[slot]] <<- trace
        stop(error)
      })
      trace$result <- cell(returned, 1L, "float")
      callback_trace[[slot]] <<- trace
      returned
    }
    unlockBinding("project_value", callback_namespace)
    assign("project_value", observed_callback, callback_namespace)
    lockBinding("project_value", callback_namespace)
  }
  access <- function(name) {
    row$host_view_access_counts[[name]] <<- if (is.null(row$host_view_access_counts[[name]])) 1L else row$host_view_access_counts[[name]] + 1L
    result[[name]]
  }
  tryCatch({
    if (file.exists(target)) stop("fresh Build directory already has an output")
    row$native_call_order <- c(row$native_call_order, list("public_build"))
    result <- timed(state, "public_build", if (is.null(declaration$environment)) domain(declaration$entry)
                   else domain(declaration$entry, environment = declaration$environment))
    retain_reached("build")
    if (!is.null(declaration$artifact_names)) {
      presence <- lapply(declaration$artifact_names, function(name) file.exists(file.path(declaration$work_directory, name)))
      names(presence) <- unlist(declaration$artifact_names)
      row$build_artifact_presence <- presence
    }
    if (length(secondary_conditions)) stop(secondary_conditions[[1L]])
    row$native_call_order <- c(row$native_call_order, list("metadata_after_build"))
    row$metadata <- timed(state, "metadata_after_build", metadata())
    views["output"] <- list(timed(state, "copied_output", frame(access("output"))))
    persist()
    views$issues <- timed(state, "copied_issues", issues(access("issues")))
    persist()
    # Use [name] assignment to preserve explicit JSON null in a named list.
    views["verification_log"] <- list(timed(state, "copied_verification_log", frame(access("verification_log"))))
    persist()
    views["warning_log"] <- list(timed(state, "copied_warning_log", frame(access("warning_log"))))
    persist()
    for (number in 1:2) {
      label <- paste0("save_", number)
      returned <- tryCatch(timed(state, paste0("held_", label), result$save()),
                           finally = retain_reached(label, include_output = TRUE))
      row$save_returns <- c(row$save_returns, list(returned))
      if (length(secondary_conditions)) stop(secondary_conditions[[1L]])
      persist()
    }
    views$output_after_save_2 <- frame(access("output"))
    row$status <- "success"
  }, error = function(error) { row$failure <<- failure(error) },
     interrupt = function(error) { row$failure <<- failure(error); interrupted <<- TRUE })
  if (!is.null(callback_namespace)) {
    unlockBinding("project_value", callback_namespace)
    assign("project_value", original_callback, callback_namespace)
    lockBinding("project_value", callback_namespace)
    row$callback_observation <- list(provider = "once-delegating installed callable wrapper",
      scope = "activation tests and derivation combined; stage split unavailable",
      count = length(callback_trace), trace = callback_trace)
  }
  if (!is.null(row$failure)) message(row$failure$message)
  if (interrupted) response$failure <- row$failure
  final_transport_failed <- FALSE
  final_transport_failure <- function(error) {
    secondary_failure(error, "final_response")
    row$status <<- "failure"
    if (is.null(row$failure)) row$failure <<- failure(error)
    row$durations <<- state$durations
    response$repetitions[[slot]] <<- row
    response$failure <<- row$failure
    message("secondary final response transport failure: ", conditionMessage(error))
    final_transport_failed <<- TRUE
  }
  tryCatch(persist(), error = final_transport_failure,
           interrupt = final_transport_failure, finally = { result <- NULL })
  if (interrupted || final_transport_failed) {
    # No retry: the controller retains stderr and the last partial response.
    quit(status = 1L)
  }
}
quit(status = if (all(vapply(response$repetitions, function(x) identical(x$status, "success"), TRUE))) 0L else 1L)
