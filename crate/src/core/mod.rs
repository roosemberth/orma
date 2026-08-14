//! Sans-IO decision logic. This crate defers all I/O to the caller.
//!
//! The operations here process inputs into verdicts or suspense points.
//! This crate makes no side-effects and is meant to be testable without
//! access to the target platform. The caller (platform) is responsible
//! of executing the side-effects and resuming across the suspense points.
//!
//! Lints enforce no-panic: By not interacting with the world, we should
//! not have any reason to panic!

#![deny(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::panic,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::exit,
        clippy::print_stdout,
        clippy::print_stderr,
        clippy::dbg_macro
    )
)]

pub mod field_type;
pub mod generate;
pub mod resolve;
pub mod schema;
