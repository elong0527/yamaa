//! An outside crate that depends on the `yamaa` facade by path, as a project
//! that embeds it does. It has no code of its own: its tests build the facade
//! and run the vendored YAML parser's compact-flow controls through it.
