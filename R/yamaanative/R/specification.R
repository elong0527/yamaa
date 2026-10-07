.specification_text_bytes <- function(value, maximum) {
  if (!is.character(value) || length(value) != 1L || is.na(value) ||
      !is.null(attributes(value))) {
    stop("expected one unclassed non-missing character scalar", call. = FALSE)
  }
  if (nchar(value, type="bytes") > maximum) stop("specification text limit", call. = FALSE)
  bytes <- .scalar_text_bytes(value)
  if (length(bytes) > maximum) stop("specification text limit", call. = FALSE)
  bytes
}

#' Prepare a bounded original specification through shared Rust
#'
#' Raw schema modules and entry YAML are captured by value. The current compiler
#' supports one typed CSV driver, source/compute columns and a closed row/SUM
#' slice. Other semantics are explicitly rejected. Preparation does not perform IO
#' or change the default runtime. No R semantic model or Python process is used.
#' @param modules Named list of raw YAML schema module snapshots.
#' @param entry Name of the entry schema module.
#' @param identity Identity of the original specification source.
#' @param source Raw original specification YAML.
#' @return An owned native specification handle.
#' @export
prepare_specification <- function(modules, entry, identity, source) {
  args <- .specification_inputs(modules, entry, identity, source)
  result <- .Call(wrap__prepare_specification, args$names, args$modules,
                  args$entry, args$identity, args$source)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}

.specification_inputs <- function(modules, entry, identity, source) {
  if (!is.list(modules) || length(modules) > 128L || is.null(names(modules)) ||
      anyNA(names(modules)) || any(!nzchar(names(modules))) ||
      anyDuplicated(names(modules)) || any(!vapply(modules, is.raw, logical(1)))) {
    stop("modules must be named raw YAML snapshots", call. = FALSE)
  }
  if (sum(vapply(modules, length, numeric(1))) > 16777216 ||
      sum(nchar(names(modules), type="bytes")) > 65536) {
    stop("captured schema limit", call. = FALSE)
  }
  .specification_text_bytes(entry, 65536)
  position <- match(entry, names(modules))
  if (is.na(position)) stop("entry schema module is absent", call. = FALSE)
  if (!is.raw(source) || length(source) > 16777216) stop("source must be bounded raw YAML", call. = FALSE)
  list(names=lapply(names(modules), .specification_text_bytes, maximum=65536),
       modules=unname(modules), entry=as.integer(position - 1L),
       identity=.specification_text_bytes(identity,65536), source=source)
}

# Internal prototype. Only raw capture, canonical identity and path authority live in R.
.prepare_inherited_specification <- function(modules, entry, identity, source,
    canonicalize, capture, rebase) {
  args <- .specification_inputs(modules, entry, identity, source)
  .prepare_with_ports(canonicalize,capture,rebase,function(dispatch) {
    .Call(wrap__prepare_inherited_specification,args$names,args$modules,
          args$entry,args$identity,args$source,dispatch)
  })
}

# Package-owned schema; native code chooses the raw-document preparation lifecycle.
.prepare_document <- function(identity, source, canonicalize, capture, rebase) {
  identity <- .specification_text_bytes(identity,65536)
  if (!is.raw(source) || length(source)>16777216) stop("source must be bounded raw YAML",call.=FALSE)
  .prepare_with_ports(canonicalize,capture,rebase,function(dispatch) {
    .Call(wrap__prepare_document,identity,source,dispatch)
  })
}

.prepare_with_ports <- function(canonicalize,capture,rebase,invoke) {
  if (!is.function(canonicalize) || !is.function(capture) || !is.function(rebase)) {
    stop("inheritance ports must be functions", call.=FALSE)
  }
  force(canonicalize); force(capture); force(rebase)
  failure <- NULL
  dispatch <- function(operation, args, maximum) tryCatch({
    strings <- lapply(args, rawToChar)
    if (operation == "canonicalize") {
      value <- canonicalize(strings[[1L]], strings[[2L]])
      if (!is.null(value)) {
        if (!is.character(value) || length(value)!=2L || anyNA(value) || !is.null(attributes(value))) {
          stop("invalid inheritance identity",call.=FALSE)
        }
        value <- lapply(value,.specification_text_bytes,maximum=maximum)
      }
    } else if (operation == "capture") {
      value <- capture(strings[[1L]], strings[[2L]], maximum)
      if (!is.null(value) && (!is.raw(value) || length(value)>maximum)) {
        stop("invalid or over-limit inheritance capture",call.=FALSE)
      }
    } else if (operation == "rebase") {
      value <- .specification_text_bytes(rebase(strings[[1L]],strings[[2L]],strings[[3L]],maximum),maximum)
    } else stop("invalid inheritance operation",call.=FALSE)
    list(0L,value)
  }, error=function(e) {failure <<- e;list(1L,NULL)},
     interrupt=function(e) {failure <<- e;list(1L,NULL)})
  result <- invoke(dispatch)
  if (!is.null(failure)) stop(failure)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}

#' Inspect the source request of a prepared specification
#' @param handle An owned handle from prepare_specification().
#' @return Source name and authored path; this function does not perform IO.
#' @export
specification_source <- function(handle) {
  result <- .Call(wrap__specification_source, handle)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}

#' Observe a failed original-specification run through an explicit source port
#'
#' The capture callback owns filesystem authorization, immutable byte capture,
#' cache policy and source verification. It takes name, path and a maximum byte
#' count, returning list(raw bytes, logical newly_created). It must bound reading
#' before allocation. Original errors and interruptions are rethrown after Rust
#' returns. The shared compiler owns parsing, binding, execution and observations.
#' Successful publication, inherited inputs and complete language coverage are not
#' implemented by this bounded failure-report entrypoint.
#' @param handle An owned prepared specification handle.
#' @param capture An explicit source capture callback.
#' @param example The report example identity.
#' @param specification Relative specification identity for observations.
#' @param base_directory Relative source base identity for observations.
#' @return Complete portable failure-report JSON; never accepted output artifacts.
#' @export
specification_failure_report <- function(handle, capture, example,
    specification = "spec.yaml", base_directory = ".") {
  .specification_observed_report(handle, capture, NULL, example, specification, base_directory)
}

#' Execute and publish a bounded original specification through shared Rust
#' @param handle An owned prepared specification handle.
#' @param capture A bounded source snapshot callback.
#' @param publish Callback taking path and raw content. It owns authorization and
#'   atomic replacement and must return TRUE only after all bytes have been
#'   published. Any other return value rejects publication.
#' @param example The report example identity.
#' @param specification Relative specification identity.
#' @param base_directory Relative source base identity.
#' @return Portable report JSON; publication errors and interrupts are rethrown.
#' @export
specification_report <- function(handle, capture, publish, example,
    specification = "spec.yaml", base_directory = ".") {
  if (!is.function(publish)) stop("publish must be a function", call. = FALSE)
  .specification_observed_report(handle, capture, publish, example, specification, base_directory)
}

.specification_observed_report <- function(handle, capture, publish, example, specification, base_directory, build=FALSE) {
  if (!is.function(capture)) stop("capture must be a function", call. = FALSE)
  force(capture)
  failure <- NULL
  dispatch <- function(name, path, maximum) tryCatch({
    result <- capture(name, path, maximum)
    if (!is.list(result) || length(result) != 2L || !is.raw(result[[1L]]) ||
        !is.logical(result[[2L]]) || length(result[[2L]]) != 1L ||
        is.na(result[[2L]]) || length(result[[1L]]) > maximum) {
      stop("invalid or over-limit source capture response", call. = FALSE)
    }
    result
  }, error = function(e) {
    failure <<- e
    list(NULL, FALSE)
  }, interrupt = function(e) {
    failure <<- e
    list(NULL, FALSE)
  })
  metadata <- lapply(list(as.character(getRversion()), engine_info()$core_version,
                          example, specification, base_directory), .specification_text_bytes, maximum=4096)
  if (build) {
    result <- .Call(wrap__specification_build, handle, dispatch, metadata)
  } else if (is.null(publish)) {
    result <- .Call(wrap__specification_failure_report, handle, dispatch, metadata)
  } else {
    force(publish)
    publish_dispatch <- function(path, content) tryCatch({
      identical(publish(path, content), TRUE)
    }, error = function(e) {
      failure <<- e
      FALSE
    }, interrupt = function(e) {
      failure <<- e
      FALSE
    })
    result <- .Call(wrap__specification_report, handle, dispatch, publish_dispatch, metadata)
  }
  if (!is.null(failure)) stop(failure)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}

# Internal owned-result API; building captures/evaluates, saving only publishes.
.specification_build <- function(handle,capture,example,specification="spec.yaml",base_directory=".") {
  .specification_observed_report(handle,capture,NULL,example,specification,base_directory,build=TRUE)
}
.build_output <- function(handle) {
  result <- .Call(wrap__build_output,handle)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.build_observations <- function(handle) {
  result <- .Call(wrap__build_observations,handle)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
.build_save <- function(handle,publish) {
  if (!is.function(publish)) stop("publish must be a function",call.=FALSE)
  force(publish)
  failure <- NULL
  dispatch <- function(path,content) tryCatch(identical(publish(path,content),TRUE),
    error=function(e) {failure <<- e;FALSE},interrupt=function(e) {failure <<- e;FALSE})
  result <- .Call(wrap__build_save,handle,dispatch)
  if (!is.null(failure)) stop(failure)
  if (!is.null(result$error)) stop(result$error,call.=FALSE)
  result$value
}
