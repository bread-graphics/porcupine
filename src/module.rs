// Boost/Apache2 License

//! Handling library modules.

use core::ptr;
use core::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

/// Get the current module for this library.
pub(crate) fn current_module() -> HINSTANCE {
    static MODULE: AtomicIsize = AtomicIsize::new(0);

    let mut module = MODULE.load(Ordering::Relaxed);

    // Load the module if it hasn't been loaded yet.
    if module == 0 {
        let our_module = unsafe { GetModuleHandleW(ptr::null()) };

        // This function should never fail.
        assert!(
            our_module != 0,
            "GetModuleHandleA failed to load the current module"
        );

        // Store the module.
        match MODULE.compare_exchange(module, our_module, Ordering::SeqCst, Ordering::Relaxed) {
            Ok(_) => module = our_module,
            Err(m) => {
                // Someone else beat us to it; just use theirs.
                module = m;
            }
        }
    }

    module
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_module() {
        let module = current_module();

        assert!(module != 0);
    }
}
