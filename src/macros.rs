// src/macros.rs

//
// Logging macros
//
// logging feature enabled → tracing
// logging feature disabled → only log_error prints to stderr
//

#![allow(unused_macros)]

// --------------------
// ERROR
// --------------------

#[cfg(feature = "logging")]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! log_error {
    ($($arg:tt)*) => {
        eprintln!($($arg)*)
    };
}

// --------------------
// WARN
// --------------------

#[cfg(feature = "logging")]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! log_warn {
    ($($arg:tt)*) => {};
}

// --------------------
// INFO
// --------------------

#[cfg(feature = "logging")]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! log_info {
    ($($arg:tt)*) => {};
}

// --------------------
// DEBUG
// --------------------

#[cfg(feature = "logging")]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        tracing::debug!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

pub(crate) use log_debug;
pub(crate) use log_error;
pub(crate) use log_info;
pub(crate) use log_warn;
