// SPDX-License-Identifier: Apache-2.0
//! `vyges-remap` as a library: only its command-line entry, for the suite binaries.

/// The `vyges-remap` command-line entry (the binary's `main`), built into the library too so a
/// suite binary can link every engine and run the one it is invoked as.
#[cfg(feature = "suite")]
#[doc(hidden)]
#[path = "main.rs"]
pub mod cli_main;
