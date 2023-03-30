// Boost/Apache2 License

#![cfg(windows)]
#![cfg_attr(not(feature = "std"), no_std)]
#![deprecated = "Use the `winsafe` crate instead"]
#![forbid(future_incompatible, rust_2018_idioms)]
#![allow(clippy::uninlined_format_args, clippy::too_many_arguments)]

//! A windowing system implementation based on the Win32 API.

extern crate alloc;

/// Convenience macro for aborting with a message.
macro_rules! abort {
    ($($arg:tt)*) => {
        $crate::abort_on_panic(move || {
            $crate::abort_with_message(&format_args!($($arg)*))
        })
    };
}

// Public modules.
pub mod bitmap;
pub mod class;
pub mod dc;
pub mod event;
pub mod gdi_object;
pub mod menu;
pub mod reactor;
pub mod region;
pub mod window;

// Private modules.
mod module;
mod wndproc;

mod client;
pub use client::Client;

use core::fmt;

use windows_sys::Win32::Foundation::GetLastError;

// On post-1.64, CStr is in core.
#[cfg(not(porcupine_no_cstr_in_core))]
mod cstr {
    pub(crate) use alloc::ffi::CString;
    pub(crate) use core::ffi::CStr;
}

// On pre-1.64, CStr is in std, we need to import libstd.
#[cfg(porcupine_no_cstr_in_core)]
mod cstr {
    extern crate std;

    pub(crate) use std::ffi::{CStr, CString};
}

/// The error type for the Win32 windowing system.
#[derive(Debug)]
pub struct Error {
    /// The error code associated with this error.
    code: u32,

    /// The message associated with this error.
    #[cfg(feature = "alloc")]
    message: Option<alloc::boxed::Box<str>>,

    /// The function that caused this error.
    function: &'static str,
}

impl Error {
    /// Get the latest error code.
    fn last_error(function: &'static str) -> Self {
        // Fetch the error code.
        let code = unsafe { GetLastError() };

        // If applicable, fetch the error message.
        #[cfg(feature = "alloc")]
        let message = {
            use core::ptr;
            use windows_sys::Win32::System::Diagnostics::Debug::FormatMessageA;
            use windows_sys::Win32::System::Diagnostics::Debug::{
                FORMAT_MESSAGE_ARGUMENT_ARRAY, FORMAT_MESSAGE_FROM_SYSTEM,
                FORMAT_MESSAGE_IGNORE_INSERTS,
            };

            // Allocate a buffer for the message.
            const BUF_SIZE: usize = 1024;
            let mut buffer = [0u8; BUF_SIZE];

            // Fetch the message.
            let mut chars_written = unsafe {
                FormatMessageA(
                    FORMAT_MESSAGE_IGNORE_INSERTS
                        | FORMAT_MESSAGE_FROM_SYSTEM
                        | FORMAT_MESSAGE_ARGUMENT_ARRAY,
                    ptr::null(),
                    code,
                    0,
                    buffer.as_mut_ptr(),
                    BUF_SIZE as u32,
                    ptr::null(),
                )
            };

            // If we failed to fetch the message, return None.
            if chars_written == 0 {
                None
            } else {
                // Trim the trailing newline.
                chars_written = chars_written.saturating_sub(2);
                let buffer = &buffer[..chars_written as usize];

                // Convert the buffer to a string.
                Some(
                    String::from_utf8_lossy(buffer)
                        .into_owned()
                        .into_boxed_str(),
                )
            }
        };

        Self {
            code,
            #[cfg(feature = "alloc")]
            message,
            function,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed", self.function)?;

        #[cfg(feature = "alloc")]
        if let Some(message) = &self.message {
            write!(f, ": {}", message)?;
        }

        write!(f, " (error code: {})", self.code)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

mod strict {
    #![allow(clippy::useless_transmute, clippy::transmutes_expressible_as_ptr_casts)]

    //! Strict provenance polyfill.

    use core::mem;

    /// Create an invalid pointer from an `isize`.
    #[inline]
    pub(crate) fn invalid(val: isize) -> *const () {
        unsafe { mem::transmute(val) }
    }

    /// Get the address of a pointer as an `isize`.
    #[inline]
    pub(crate) fn addr(ptr: *const ()) -> isize {
        unsafe { mem::transmute(ptr) }
    }

    /// Expose the address of a pointer.
    #[inline]
    pub(crate) fn expose(ptr: *const ()) -> isize {
        addr(ptr)
    }

    /// Reconstitute a pointer from an `isize`.
    #[inline]
    pub(crate) fn reconstitute(val: isize) -> *const () {
        invalid(val)
    }
}

fn abort() -> ! {
    #[cfg(feature = "std")]
    std::process::abort();

    #[cfg(not(feature = "std"))]
    {
        // In Rust, panicking while panicking is defined as causing an abort.
        struct Abort;

        impl Drop for Abort {
            fn drop(&mut self) {
                panic!("panic while panicking");
            }
        }

        let _abort = Abort;
        panic!("panic while panicking to abort the process");
    }
}

fn abort_with_message(msg: &fmt::Arguments<'_>) -> ! {
    /// tracing::error! may panic, so we need to abort if that happens.
    struct AbortOnDrop;

    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            abort();
        }
    }

    let _bomb = AbortOnDrop;
    tracing::error!("Aborting: {}", msg);
    abort()
}

fn abort_on_panic<R>(f: impl FnOnce() -> R) -> R {
    struct AbortOnPanic;

    impl Drop for AbortOnPanic {
        fn drop(&mut self) {
            abort!("Function panicked in a context where panics are not allowed");
        }
    }

    let _abort_on_panic = AbortOnPanic;
    let result = f();
    core::mem::forget(_abort_on_panic);
    result
}
