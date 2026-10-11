# The installed R owner runs original YAML with no Python executable available.
original_path <- Sys.getenv("PATH")
runtime_path <- tempfile("producer-native-only-");dir.create(runtime_path)
rm_path <- Sys.which("rm")
if(nzchar(rm_path)) invisible(file.symlink(rm_path,file.path(runtime_path,"rm")))
Sys.setenv(PATH=runtime_path)
stopifnot(!nzchar(Sys.which("python")),!nzchar(Sys.which("python3")))
library(yamaa)
root <- system.file("project-original",package="yamaa",mustWork=TRUE)
name <- "shared-producer-parquet"
work <- tempfile("producer-cohort-");dir.create(work)
work <- normalizePath(work,winslash="/",mustWork=TRUE)
case <- file.path(work,name);dir.create(case)
for(input in c("seed.csv","producer.yaml","spec.yaml"))
  stopifnot(file.copy(file.path(root,name,input),file.path(case,input)))
writeBin(charToRaw("version: '1.0'\n"),file.path(work,"yamaa-project.yaml"))
writeBin(charToRaw("schema_version: '1.0'\nlanguage: r\nlock: renv.lock\nfunctions: {}\n"),file.path(work,"environment.yaml"))
stopifnot(file.copy(file.path(root,"schema-functions/r/renv.lock"),file.path(work,"renv.lock")))
output <- Sys.getenv("YAMAA_PUBLIC_REPORT_DIR")
evidence <- if(nzchar(output)) file.path(output,"producer-cohort/r",name) else file.path(work,"evidence")
dir.create(evidence,recursive=TRUE,showWarnings=FALSE)
writeBin(charToRaw(case),file.path(evidence,"case-directory.txt"))
provenance <- Sys.getenv("YAMAA_SUPPLEMENTAL_EVIDENCE")
if(nzchar(provenance)) writeBin(charToRaw(provenance),file.path(evidence,"provenance.json"))
rawfile <- function(path) readBin(path,"raw",n=file.info(path)$size)
observe <- get(".domain_observations",envir=asNamespace("yamaa"))
result <- yamaa_domain(file.path(case,"spec.yaml"),environment=file.path(work,"environment.yaml"))
stopifnot(nrow(result$issues)==0L,identical(result$output$ID,"01"))
before <- observe(result)
writeBin(before,file.path(evidence,"before.json"))
stopifnot(!file.exists(file.path(case,"produced.parquet")),!file.exists(file.path(case,"out.csv")))
unlink(file.path(case,c("seed.csv","producer.yaml","spec.yaml")))
saved <- NULL
for(attempt in 1:2) {
  stopifnot(isTRUE(result$save()),identical(result$output$ID,"01"))
  directory <- file.path(evidence,paste0("after-",attempt));dir.create(directory)
  writeBin(observe(result),file.path(directory,"report.json"))
  actual <- lapply(c("produced.parquet","out.csv"),function(target) {
    bytes <- rawfile(file.path(case,target))
    writeBin(bytes,file.path(directory,target))
    bytes
  })
  stopifnot(identical(actual[[2L]],charToRaw("ID\n01\n")),
    identical(actual[[1L]][1:4],charToRaw("PAR1")),
    identical(tail(actual[[1L]],4L),charToRaw("PAR1")))
  if(!is.null(saved)) stopifnot(identical(actual,saved))
  saved <- actual
}
unlink(work,recursive=TRUE)
Sys.setenv(PATH=original_path)
unlink(runtime_path,recursive=TRUE)
cat("shared-producer-parquet actual installed public owner and repeated held-byte Save evidence passed\n")
