// Boost/Apache2 License

//! Generalized GDI object types.

use core::cell::Cell;
use core::marker::PhantomData;
use core::mem;
use core::num::NonZeroIsize;

use windows_sys::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};

/// Raw GDI object.
pub type RawGdiObject = HGDIOBJ;

/// An owned GDI object.
#[repr(transparent)]
pub struct OwnedGdiObject {
    /// The handle to the GDI object.
    handle: NonZeroIsize,

    /// This handle is `Send` but `!Sync`.
    _thread_safety: PhantomData<Cell<()>>,

    /// This handle is an HGDIOBJ.
    _gdi_object: PhantomData<HGDIOBJ>,
}

impl Drop for OwnedGdiObject {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.handle.get() as _);
        }
    }
}

impl OwnedGdiObject {
    /// Creates a new owned GDI object.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid handle to a GDI object.
    pub unsafe fn new(handle: RawGdiObject) -> Self {
        Self {
            handle: NonZeroIsize::new_unchecked(handle as _),
            _thread_safety: PhantomData,
            _gdi_object: PhantomData,
        }
    }

    /// Consumes the owned GDI object and returns the underlying handle.
    ///
    /// # Safety
    ///
    /// The user must delete the object or it will be leaked.
    pub fn into_handle(self) -> RawGdiObject {
        let handle = self.handle.get() as _;
        mem::forget(self);
        handle
    }
}

/// A borrowed GDI object.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct BorrowedGdiObject<'a> {
    /// The handle to the GDI object.
    handle: NonZeroIsize,

    /// This handle is represented by a `&'a OwnedGdiObject`.
    _marker: PhantomData<&'a OwnedGdiObject>,
}

impl<'a> BorrowedGdiObject<'a> {
    /// Creates a new borrowed GDI object.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid handle to a GDI object.
    pub unsafe fn new(handle: RawGdiObject) -> Self {
        Self {
            handle: NonZeroIsize::new_unchecked(handle as _),
            _marker: PhantomData,
        }
    }
}

/// A trait that allows one to borrow a GDI object.
pub trait AsGdiObject {
    /// Borrows the GDI object.
    fn as_gdi_object(&self) -> BorrowedGdiObject<'_>;
}

impl AsGdiObject for OwnedGdiObject {
    fn as_gdi_object(&self) -> BorrowedGdiObject<'_> {
        unsafe { BorrowedGdiObject::new(self.handle.get() as _) }
    }
}

impl<'a> AsGdiObject for BorrowedGdiObject<'a> {
    fn as_gdi_object(&self) -> BorrowedGdiObject<'_> {
        *self
    }
}
