#' Invoke one explicit callback through the shared Rust engine
#'
#' The function/1 request describes normalized arguments and an exact signature.
#' Caller-supplied identity labels do not authenticate an artifact. This bounded
#' prototype does not activate an environment or execute a specification.
#' Integers and strings use lossless yamaa_int64/yamaa_utf8 scalars; temporal
#' arguments use Date or UTC POSIXct and drop collected precision at this boundary.
#' @param request One unclassed, nonmissing JSON character string.
#' @param callback An explicitly supplied function, called once on the R thread.
#' @return Owned outcome JSON. Transport failures raise after native return.
#'   Ordinary callback errors and invalid returns are distinct fatal outcomes.
#' @export
invoke_function <- function(request, callback) {
  if (!is.function(callback)) stop("callback must be a function", call. = FALSE)
  state <- .function_dispatcher(callback)
  result <- .Call(wrap__invoke_function, .scalar_text_bytes(request), state$dispatch)
  interrupted <- state$interruption()
  if (!is.null(interrupted)) stop(interrupted)
  if (!is.null(result$error)) stop(result$error, call. = FALSE)
  result$value
}

#' Build one stable callback dispatcher and retain its original interruption condition.
.function_dispatcher <- function(callback) {
  force(callback)
  interrupted <- NULL
  dispatch <- function(encoded) tryCatch({
    args <- lapply(encoded, .scalar_unpack)
    called <- tryCatch(
      list(ok = TRUE, value = do.call(callback, args, quote = TRUE)),
      error = function(e) list(ok = FALSE, result = list(
        1L, .function_detail(function() class(e)[1L], "R condition"),
        .function_detail(function() conditionMessage(e), "unavailable R condition message")))
    )
    if (!called$ok) return(called$result)
    # Return representation admission is outside the callback's error handler.
    tryCatch({
      value <- .function_pack(called$value)
      list(0L, value$tag, value$payload)
    }, error = function(e) {
      if (identical(conditionMessage(e), "scalar exceeds byte limit")) {
        return(list(4L, NULL, NULL))
      }
      list(2L, charToRaw("a binding returned a value of no scalar type"),
           charToRaw(typeof(called$value)))
    })
  }, interrupt = function(e) {
    interrupted <<- e
    list(3L, NULL, NULL)
  })
  list(dispatch = dispatch, interruption = function() interrupted)
}

#' Keep failures in secondary condition rendering from replacing the primary error.
.function_detail <- function(read, fallback) {
  tryCatch(.scalar_text_bytes(read()), error = function(e) charToRaw(fallback),
           interrupt = function(e) charToRaw(fallback))
}

#' Admit designated scalar storage and exact temporal host representations.
.function_pack <- function(x) {
  attrs <- attributes(x)
  date <- identical(attrs, list(class = "Date"))
  datetime <- identical(attrs, list(class = c("POSIXct", "POSIXt"), tzone = "UTC")) ||
    identical(attrs, list(tzone = "UTC", class = c("POSIXct", "POSIXt")))
  if (!date && !datetime) return(.scalar_pack(x))
  epoch <- unclass(x)
  attributes(epoch) <- NULL
  if (!(typeof(epoch) %in% c("integer", "double")) || length(epoch) != 1L) {
    stop("invalid temporal scalar storage", call. = FALSE)
  }
  if (is.na(epoch) || !is.finite(epoch)) return(.scalar_native(0L, raw()))
  .scalar_native(if (date) 5L else 6L,
                 writeBin(as.double(epoch), raw(), size = 8L, endian = "little"))
}
