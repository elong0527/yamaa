//! Service contracts in the yamaa-engine integration suite.

mod build_metadata;
#[cfg(feature = "test-support")]
mod build_ports;
mod define_service;
mod domain_entry;
mod specification_odm;
mod specification_run;
mod submission_inventory;
