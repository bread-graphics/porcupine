// Boost/Apache2 License

//! Win32 regions.

use core::cell::Cell;
use core::marker::PhantomData;

use windows_sys::Win32::Graphics::Gdi::DeleteObject;
use windows_sys::Win32::Graphics::Gdi::HRGN;

/// A Win32 region.
pub struct Region {
    /// The handle to the region.
    handle: HRGN,

    /// This handle is `Send` but `!Sync`.
    thread_safety: PhantomData<Cell<()>>,
}

impl Region {
    pub(crate) fn into_handle(self) -> HRGN {
        let handle = self.handle;
        core::mem::forget(self);
        handle
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.handle);
        }
    }
}
