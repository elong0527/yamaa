# Codec only: no installed/native/reference evaluator is used to author truth.
# A suite sources this file once and may read several families, so the
# document is decoded once per suite, however many families it reads.
host_fixture_documents <- new.env(parent = emptyenv())
read_host_fixture <- function(root, family) {
  document <- host_fixture_documents[[root]]
  if (is.null(document)) {
    document <- jsonlite::fromJSON(file.path(root,"cases.json"),simplifyVector=FALSE)
    if(!identical(document$version,"1")) stop("unsupported installed-host fixture version")
    assign(root, document, envir = host_fixture_documents)
  }
  value <- document$families[[family]]
  if(is.null(value)) stop("unknown installed-host fixture family: ",family)
  value
}
fixture_strings <- function(value) unlist(value,use.names=FALSE)
fixture_bytes <- function(value) {
  if(!is.list(value)) return(charToRaw(enc2utf8(value)))
  stopifnot("a byte fixture is text or hexadecimal text"=identical(names(value),"hex"))
  if(!nzchar(value$hex)) return(raw())
  starts <- seq.int(1L,nchar(value$hex),by=2L)
  as.raw(strtoi(substring(value$hex,starts,starts+1L),base=16L))
}
