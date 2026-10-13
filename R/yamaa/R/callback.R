# Keep secondary condition rendering from replacing a retained callback failure.
.function_detail <- function(read, fallback) {
  tryCatch(.scalar_text_bytes(read()), error = function(e) charToRaw(fallback),
           interrupt = function(e) charToRaw(fallback))
}

# Marshal designated scalar classes and temporal storage for production callbacks.
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
  # Shared scalar admission owns nonfinite normalization before temporal admission.
  admitted <- .scalar_native(2L, writeBin(as.double(epoch), raw(), size = 8L, endian = "little"))
  if (admitted$tag == 0L) return(admitted)
  .scalar_native(if (date) 5L else 6L, admitted$payload)
}
