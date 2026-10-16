# Host authority is a callback; native code owns snapshots and source semantics.
#' Return one host resource result
#' @param reference Immutable opaque object identity, without credentials.
#' @param data Raw captured bytes, or NULL outside capture.
#' @param frame An Arrow-compatible immutable frame, or NULL.
#' @export
yamaa_resource_result <- function(reference, data = NULL, frame = NULL) {
  .domain_path_bytes(reference)
  if (!is.null(data) && !is.raw(data)) stop("resource data must be raw bytes")
  structure(list(reference = reference, data = data, frame = frame),
    class = "yamaa_resource_result")
}

#' Report an explicitly classified resource failure
#' @param reason One of missing, changed, cancelled, rejected, publication, cleanup.
#' @export
yamaa_resource_failure <- function(reason) {
  if (!(reason %in% c("missing", "changed", "cancelled", "rejected", "publication", "cleanup"))) {
    stop("unknown resource failure")
  }
  stop(structure(list(message = reason, call = NULL, reason = reason),
    class = c("yamaa_resource_failure", "error", "condition")))
}

#' Adapt a host callback to metadata, input and explicit Save resource ports
#' @param callback A function accepting a typed request list and returning a resource result.
#' @export
yamaa_resource_resolver <- function(callback) {
  stopifnot(is.function(callback))
  state <- new.env(parent = emptyenv())
  state$pending <- NULL
  state$provenance <- list()
  provenance_keys <- list()
  state$request <- callback
  state$session <- function() yamaa_resource_resolver(callback)
  state$raise_pending <- function() {
    error <- state$pending
    state$pending <- NULL
    if (!is.null(error)) stop(error)
  }
  state$dispatch <- function(request) {
    tryCatch({
      request$declaring <- rawToChar(request$declaring)
      request$written <- rawToChar(request$written)
      if (!is.null(request$reference)) request$reference <- rawToChar(request$reference)
      class(request) <- "yamaa_resource_request"
      reply <- callback(request)
      if (!inherits(reply, "yamaa_resource_result")) yamaa_resource_failure("rejected")
      if (request$operation == "capture") {
        if (is.null(reply$data) == is.null(reply$frame)) yamaa_resource_failure("rejected")
      } else if (!is.null(reply$data) || !is.null(reply$frame)) yamaa_resource_failure("rejected")
      frame <- if (is.null(reply$frame)) NULL else if (inherits(reply$frame, "yamaa_owned_frame")) {
        nanoarrow::basic_array_stream(reply$frame$batches, schema = reply$frame$schema)
      } else nanoarrow::as_nanoarrow_array_stream(reply$frame)
      key <- list(request$declaring, request$written, request$kind)
      if (identical(request$operation, "capture") &&
          !any(vapply(provenance_keys, identical, logical(1), key))) {
        record <- list(declaring = request$declaring, written = request$written,
          kind = request$kind, reference = reply$reference,
          representation = if (is.null(reply$frame)) "bytes" else "frame")
        state$provenance[[length(state$provenance) + 1L]] <- record
        provenance_keys[[length(provenance_keys) + 1L]] <<- key
      }
      list(failure = NULL, reference = charToRaw(enc2utf8(reply$reference)),
        data = reply$data, frame = frame)
    }, yamaa_resource_failure = function(error) list(failure = error$reason),
    error = function(error) {
      if (is.null(state$pending)) state$pending <- error
      list(failure = "rejected")
    }, interrupt = function(error) {
      if (is.null(state$pending)) state$pending <- error
      list(failure = "cancelled")
    })
  }
  class(state) <- "yamaa_resource_resolver"
  state
}

#' Create a deterministic in-memory resource backend
#' @param objects A named list of raw input objects.
#' @param prefix Opaque identity prefix for the backend.
#' @export
yamaa_memory_resolver <- function(objects = list(), prefix = "memory:/") {
  backend <- new.env(parent = emptyenv())
  backend$objects <- list()
  backend$outputs <- list()
  backend$staging <- list()
  backend$cancelled <- FALSE
  next_version <- 0L
  prefix <- paste0(sub("/+$", "", prefix), "/")
  backend$put <- function(key, data, reference = NULL, frame = NULL) {
    next_version <<- next_version + 1L
    if (is.null(reference)) reference <- paste0(prefix, key, "#v", next_version)
    if (!is.null(frame)) {
      stream <- nanoarrow::as_nanoarrow_array_stream(frame)
      schema <- stream$get_schema()
      frame <- structure(list(batches = nanoarrow::collect_array_stream(stream), schema = schema),
        class = "yamaa_owned_frame")
    }
    backend$objects[[key]] <- yamaa_resource_result(reference, data, frame)
    invisible(NULL)
  }
  for (key in names(objects)) backend$put(key, objects[[key]])
  key_for <- function(declaring, written) {
    parts <- strsplit(paste0(dirname(declaring), "/", written), "/", fixed = TRUE)[[1L]]
    if (startsWith(written, "/")) parts <- strsplit(written, "/", fixed = TRUE)[[1L]]
    result <- character()
    for (part in parts) {
      if (part %in% c("", ".")) next
      if (part == "..") { result <- utils::head(result, -1L); next }
      result <- c(result, part)
    }
    paste(result, collapse = "/")
  }
  resolver <- yamaa_resource_resolver(function(request) {
    if (request$operation == "abort") {
      backend$staging[[request$reference]] <- NULL
      for (token in names(backend$staging)) {
        if (identical(backend$staging[[token]]$reference, request$reference)) backend$staging[[token]] <- NULL
      }
      return(yamaa_resource_result(request$reference))
    }
    if (backend$cancelled) yamaa_resource_failure("cancelled")
    key <- key_for(request$declaring, request$written)
    if (request$operation %in% c("inspect", "capture")) {
      reply <- backend$objects[[key]]
      if (is.null(reply)) yamaa_resource_failure("missing")
      if (!is.null(request$reference) && !identical(reply$reference, request$reference)) yamaa_resource_failure("changed")
      return(if (request$operation == "capture") reply else yamaa_resource_result(reply$reference))
    }
    if (request$operation == "target") return(yamaa_resource_result(paste0(prefix, key)))
    if (request$operation == "stage") {
      next_version <<- next_version + 1L
      token <- paste0("stage:", next_version, ":", request$reference)
      backend$staging[[token]] <- yamaa_resource_result(request$reference, request$data)
      return(yamaa_resource_result(token))
    }
    if (request$operation == "commit") {
      staged <- backend$staging[[request$reference]]
      if (is.null(staged)) yamaa_resource_failure("publication")
      backend$outputs[[staged$reference]] <- staged$data
      backend$staging[[request$reference]] <- NULL
      return(yamaa_resource_result(staged$reference))
    }
    yamaa_resource_failure("rejected")
  })
  resolver$backend <- backend
  resolver
}

#' Create a synthetic versioned remote resolver without private services
#' @inheritParams yamaa_memory_resolver
#' @export
yamaa_synthetic_remote_resolver <- function(objects = list()) {
  yamaa_memory_resolver(objects, prefix = "object://synthetic/")
}
