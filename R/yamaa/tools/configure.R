# Find the Cargo and rustc that compile the Rust engine, and write the
# Makevars that use them. configure and configure.win run this script with the
# R that installs the package.
#
# It follows CRAN's "Using Rust in CRAN packages": it looks for the cargo and
# rustc commands on the PATH and then in ~/.cargo/bin, where rustup installs
# them and which is often not on the PATH; it requires each to be at least the
# rust-version that the bundled workspace declares in src/rust/Cargo.toml;
# it reports both versions in the installation log before compilation starts;
# and when either is missing or too old it stops and asks the user to install
# them. It never installs Rust, and RUSTUP_AUTO_INSTALL=0 stops rustup from
# installing a toolchain that a rust-toolchain.toml file names.

# The Rust target that Rtools' GCC and UCRT link on Windows.
WINDOWS_TARGET <- "x86_64-pc-windows-gnu"

# The rust-version that src/rust/Cargo.toml declares, as major.minor.patch.
minimum_rust <- function(manifest = file.path("src", "rust", "Cargo.toml")) {
  lines <- readLines(manifest, warn = FALSE)
  pattern <- '^rust-version[[:space:]]*=[[:space:]]*"([0-9]+(\\.[0-9]+){0,2})"'
  declared <- sub(paste0(pattern, ".*$"), "\\1", grep(pattern, lines, value = TRUE))
  if (length(declared) != 1L) {
    stop("cannot read the rust-version that ", manifest, " declares", call. = FALSE)
  }
  parts <- c(strsplit(declared, ".", fixed = TRUE)[[1L]], "0", "0")
  paste(parts[1:3], collapse = ".")
}

# The directories of personal installations: rustup's Cargo home, which
# CARGO_HOME names when it is set, and otherwise ~/.cargo.
personal_directories <- function(windows) {
  homes <- Sys.getenv(if (windows) c("USERPROFILE", "HOME") else "HOME")
  directories <- c(
    if (nzchar(Sys.getenv("CARGO_HOME"))) file.path(Sys.getenv("CARGO_HOME"), "bin"),
    file.path(homes[nzchar(homes)], ".cargo", "bin")
  )
  unique(directories)
}

# The path of a command on the PATH, or else in a personal installation, or
# "" when there is none. Links are kept: a rustup proxy chooses its tool by
# the name it is run as.
locate <- function(command, windows) {
  name <- if (windows) paste0(command, ".exe") else command
  path <- strsplit(Sys.getenv("PATH"), .Platform$path.sep, fixed = TRUE)[[1L]]
  for (directory in c(path[nzchar(path)], personal_directories(windows))) {
    candidate <- file.path(directory, name)
    if (file.exists(candidate) && !dir.exists(candidate) &&
        file.access(candidate, 1L) == 0L) {
      # make runs in src/, so a relative PATH entry becomes absolute.
      if (!grepl("^([/\\\\]|[A-Za-z]:)", candidate)) {
        candidate <- file.path(getwd(), candidate)
      }
      return(candidate)
    }
  }
  ""
}

# The output lines of a command, with its exit status as attribute "status".
# system2() quotes the command itself.
run <- function(path, arguments) {
  output <- tryCatch(
    suppressWarnings(system2(path, arguments, stdout = TRUE, stderr = TRUE)),
    error = function(error) structure(conditionMessage(error), status = 127L)
  )
  status <- attr(output, "status")
  attr(output, "status") <- if (is.null(status)) 0L else status
  output
}

stop_with <- function(...) {
  rule <- strrep("-", 72L)
  stop(paste(c("", rule, ..., rule), collapse = "\n"), call. = FALSE)
}

install_advice <- function(windows) {
  if (windows) {
    c(
      "Install Rust with rustup from https://rustup.rs/ and add the target",
      sprintf("that Rtools links: rustup target add %s", WINDOWS_TARGET)
    )
  } else {
    c(
      "Install them with the system's package manager, for example",
      "  apt-get install cargo rustc",
      "  dnf install cargo",
      "or with rustup from https://rustup.rs/ ."
    )
  }
}

# Check one command and return its path and the first line of its version.
check_tool <- function(command, minimum, windows) {
  path <- locate(command, windows)
  if (!nzchar(path)) {
    stop_with(
      sprintf("yamaa needs Cargo and rustc %s or later to compile its Rust engine.", minimum),
      sprintf("No '%s' command was found on the PATH or in %s.", command,
              paste(personal_directories(windows), collapse = " or ")),
      install_advice(windows)
    )
  }
  output <- run(path, "--version")
  line <- grep(sprintf("^%s [0-9]+\\.[0-9]+", command), output, value = TRUE)
  if (attr(output, "status") != 0L || !length(line)) {
    stop_with(
      sprintf("'%s --version' failed for %s:", command, path),
      paste0("  ", output),
      sprintf("yamaa needs Cargo and rustc %s or later.", minimum),
      install_advice(windows)
    )
  }
  line <- line[[1L]]
  version <- regmatches(line, regexpr("[0-9]+\\.[0-9]+(\\.[0-9]+)?", line))
  if (package_version(version) < package_version(minimum)) {
    stop_with(
      sprintf("yamaa needs %s %s or later; %s is %s.", command, minimum, path, line),
      "Update Rust with the system's package manager, or with 'rustup update'",
      "for a rustup installation."
    )
  }
  list(path = path, version = line)
}

# Whether rustc has the standard library of a target.
has_target <- function(rustc, target) {
  sysroot <- trimws(run(rustc, c("--print", "sysroot")))
  if (attr(sysroot, "status") != 0L || length(sysroot) != 1L) {
    return(FALSE)
  }
  length(list.files(file.path(sysroot, "lib", "rustlib", target, "lib"),
                    pattern = "^libstd-.*[.]rlib$")) > 0L
}

# A path as make reads it in a recipe: forward slashes and "$" doubled.
make_path <- function(path) {
  gsub("$", "$$", gsub("\\", "/", path, fixed = TRUE), fixed = TRUE)
}

# The directory where Cargo builds, keeps its home and unpacks the crates, as
# the Makevars name it: CARGO_TARGET_DIR, and UNTAR_DIR for untar() on
# Windows. It is the package's own src/rust/target, which the Makevars remove
# after linking. YAMAA_R_CARGO_TARGET_DIR names another, absolute directory
# that installations share, as the project's CI does. Cargo's own
# CARGO_TARGET_DIR, which may serve other projects, is never read: the
# Makevars pass this directory to Cargo themselves.
target_directories <- function(windows) {
  directory <- Sys.getenv("YAMAA_R_CARGO_TARGET_DIR")
  if (!nzchar(directory)) {
    return(c(CARGO_TARGET_DIR = if (windows) "$(CURDIR)/rust/target" else "rust/target",
             UNTAR_DIR = "rust/target"))
  }
  # make and the shell read the path as written, so it may not hold spaces,
  # quotes or other characters that they interpret.
  if (!grepl("^([A-Za-z]:)?[/\\\\][-A-Za-z0-9._+~/\\\\]*$", directory)) {
    stop_with(
      sprintf("YAMAA_R_CARGO_TARGET_DIR is '%s'.", directory),
      "It must be an absolute path of letters, digits and . _ - + ~ / \\ only."
    )
  }
  cat(sprintf("Cargo builds in %s, which YAMAA_R_CARGO_TARGET_DIR names\n", directory))
  c(CARGO_TARGET_DIR = make_path(directory), UNTAR_DIR = make_path(directory))
}

# The settings that keep the build in the package's target directory, which
# the Makevars pass to Cargo: ISOLATION_ENV as environment variables and
# ISOLATION_CONFIG as options. Without them, the caller's environment or a
# .cargo/config.toml in a directory above the package could send Cargo's
# intermediate files elsewhere (CARGO_BUILD_BUILD_DIR or build.build-dir,
# which current Cargo reads and older Cargo ignores), or run a rustc wrapper,
# such as a compiler cache with its own cache directory (RUSTC_WRAPPER and
# RUSTC_WORKSPACE_WRAPPER, or build.rustc-wrapper and
# build.rustc-workspace-wrapper, which CARGO_BUILD_ variables also set).
# - The build directory is the target directory, Cargo's default. The
#   variable outranks every configuration file. Cargo refuses an empty build
#   directory, and an older Cargo, which does not know the key, warns about a
#   --config value for it. It is rust/target, relative to src/ on Windows
#   too: current Cargo reads the value as a template, so a { or } in an
#   absolute path, such as R's temporary directory, would stop Cargo or name
#   another directory. Cargo joins a relative value to the directory where it
#   runs only after reading it.
# - An empty RUSTC_WRAPPER or RUSTC_WORKSPACE_WRAPPER runs no wrapper; only
#   the variable itself overrides one that the caller sets. The empty
#   --config values outrank the configuration keys and their CARGO_BUILD_
#   variables, even where an empty environment variable does not reach Cargo.
#   They are TOML literal strings, so no double quote has to pass through a
#   Windows command line.
# YAMAA_R_CARGO_TARGET_DIR moves the build out of the package anyway, so with
# it set these settings apply as to any Cargo build.
isolation <- function() {
  if (nzchar(Sys.getenv("YAMAA_R_CARGO_TARGET_DIR"))) {
    return(c(ISOLATION_ENV = "", ISOLATION_CONFIG = ""))
  }
  c(
    ISOLATION_ENV = paste(
      "RUSTC_WRAPPER= RUSTC_WORKSPACE_WRAPPER=",
      "CARGO_BUILD_BUILD_DIR=rust/target"
    ),
    ISOLATION_CONFIG = paste(
      "--config \"build.rustc-wrapper=''\"",
      "--config \"build.rustc-workspace-wrapper=''\""
    )
  )
}

main <- function(windows = .Platform$OS.type == "windows",
                 darwin = Sys.info()[["sysname"]] == "Darwin") {
  Sys.setenv(RUSTUP_AUTO_INSTALL = "0")
  minimum <- minimum_rust()
  rustc <- check_tool("rustc", minimum, windows)
  cargo <- check_tool("cargo", minimum, windows)
  cat(sprintf("using %s from %s\n", rustc$version, rustc$path))
  cat(sprintf("using %s from %s\n", cargo$version, cargo$path))
  if (windows && !has_target(rustc$path, WINDOWS_TARGET)) {
    stop_with(
      sprintf("%s has no standard library for %s, the target that Rtools links.",
              rustc$path, WINDOWS_TARGET),
      sprintf("Add it, for a rustup installation with: rustup target add %s", WINDOWS_TARGET)
    )
  }
  # cc-rs otherwise selects the current SDK's minimum macOS version for the C
  # code that crates compile, which can be newer than R's link target: match
  # Rust's arm64 floor unless the caller sets one.
  environment <- if (darwin) {
    paste0("MACOSX_DEPLOYMENT_TARGET=", Sys.getenv("MACOSX_DEPLOYMENT_TARGET", "11.0"))
  } else {
    ""
  }
  makevars <- file.path("src", if (windows) "Makevars.win" else "Makevars")
  lines <- readLines(paste0(makevars, ".in"), warn = FALSE)
  values <- c(CARGO = make_path(cargo$path), RUSTC = make_path(rustc$path),
              CARGO_ENV = environment, target_directories(windows), isolation())
  for (name in names(values)) {
    lines <- gsub(sprintf("@%s@", name), values[[name]], lines, fixed = TRUE)
  }
  # A binary connection keeps LF line endings on Windows too.
  connection <- file(makevars, "wb")
  on.exit(close(connection))
  writeLines(lines, connection)
  invisible(makevars)
}

if (sys.nframe() == 0L) main()
