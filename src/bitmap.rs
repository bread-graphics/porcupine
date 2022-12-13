// Boost/Apache2 License

//! Functions for making and managing bitmaps.

use crate::gdi_object::{AsGdiObject, BorrowedGdiObject, OwnedGdiObject};
use crate::Error;

use alloc::borrow::Cow;

use core::cell::Cell;
use core::marker::PhantomData;
use core::num::{NonZeroI32, NonZeroU16};
use core::ptr::NonNull;

use windows_sys::Win32::Graphics::Gdi::{CreateBitmapIndirect, DeleteObject};
use windows_sys::Win32::Graphics::Gdi::{BITMAP, BITMAPINFOHEADER, HBITMAP};

macro_rules! nz_unchecked {
    ($ty:ty, $expr:expr) => {{
        cfg_if::cfg_if! {
            if #[cfg(debug_assertions)] {
                <$ty>::new($expr).expect("non-zero value was zero")
            } else {
                unsafe { <$ty>::new_unchecked($expr) }
            }
        }
    }};
}

/// Information about a bitmap.
pub struct BitmapInfo<'a> {
    /// The inner bitmap information.
    inner: BITMAP,

    /// The bitmap data.
    bits: Cow<'a, [u8]>,
}

/// A bitmap.
pub struct Bitmap {
    /// The handle to the bitmap.
    handle: OwnedGdiObject,

    /// This handle is `Send` but `!Sync`.
    thread_safety: PhantomData<Cell<()>>,
}

/// A device-independent bitmap.
pub struct DIBitmap {
    /// The inner bitmap information.
    handle: Bitmap,

    /// Pointer to the bitmap data.
    data: NonNull<[u8]>,
}

// SAFETY: The data is owned by the `DIBitmap`.
unsafe impl Send for DIBitmap {}

impl<'a> BitmapInfo<'a> {
    /// Create a new `BitmapInfo`.
    pub fn new(
        width: NonZeroI32,
        height: NonZeroI32,
        scanline_width: NonZeroI32,
        planes: NonZeroU16,
        bits_per_pixel: NonZeroU16,
        bits: impl Into<Cow<'a, [u8]>>,
    ) -> Self {
        Self {
            inner: BITMAP {
                bmType: 0,
                bmWidth: width.get(),
                bmHeight: height.get(),
                bmWidthBytes: scanline_width.get(),
                bmPlanes: planes.get(),
                bmBitsPixel: bits_per_pixel.get(),
                // This field is set when it's converted to a `BITMAPINFO`.
                bmBits: core::ptr::null_mut(),
            },
            bits: bits.into(),
        }
    }

    /// Get the width of the bitmap.
    pub fn width(&self) -> NonZeroI32 {
        nz_unchecked!(NonZeroI32, self.inner.bmWidth)
    }

    /// Get the height of the bitmap.
    pub fn height(&self) -> NonZeroI32 {
        nz_unchecked!(NonZeroI32, self.inner.bmHeight)
    }

    /// Get the scanline width of the bitmap.
    pub fn scanline_width(&self) -> NonZeroI32 {
        nz_unchecked!(NonZeroI32, self.inner.bmWidthBytes)
    }

    /// Get the number of planes in the bitmap.
    pub fn planes(&self) -> NonZeroU16 {
        nz_unchecked!(NonZeroU16, self.inner.bmPlanes)
    }

    /// Get the number of bits per pixel in the bitmap.
    pub fn bits_per_pixel(&self) -> NonZeroU16 {
        nz_unchecked!(NonZeroU16, self.inner.bmBitsPixel)
    }

    /// Get the bitmap data.
    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    /// Get a mutable reference to the bitmap data.
    pub fn bits_mut(&mut self) -> &mut [u8] {
        self.bits.to_mut()
    }
}

impl Bitmap {
    /// Create a new bitmap from a `BitmapInfo`.
    pub fn new(info: &BitmapInfo<'_>) -> Result<Self, Error> {
        // Setup the raw structure.
        let mut raw_bitmap = info.inner;
        let bits = info.bits.as_ptr() as *mut _;
        raw_bitmap.bmBits = bits;

        // Create the bitmap.
        let bitmap = unsafe { CreateBitmapIndirect(&raw_bitmap) };
        if bitmap == 0 {
            Err(Error::last_error("CreateBitmapIndirect"))
        } else {
            Ok(Self {
                handle: unsafe { OwnedGdiObject::new(bitmap) },
                thread_safety: PhantomData,
            })
        }
    }

    pub(crate) fn into_handle(self) -> HBITMAP {
        self.handle.into_handle()
    }
}

impl From<OwnedGdiObject> for Bitmap {
    fn from(handle: OwnedGdiObject) -> Self {
        Self {
            handle,
            thread_safety: PhantomData,
        }
    }
}

impl From<Bitmap> for OwnedGdiObject {
    fn from(bitmap: Bitmap) -> Self {
        bitmap.handle
    }
}

impl AsGdiObject for Bitmap {
    fn as_gdi_object(&self) -> BorrowedGdiObject<'_> {
        self.handle.as_gdi_object()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::borrow::Cow;

    #[test]
    fn bitmap() {
        let info = BitmapInfo::new(
            nz_unchecked!(NonZeroI32, 1),
            nz_unchecked!(NonZeroI32, 1),
            nz_unchecked!(NonZeroI32, 2),
            nz_unchecked!(NonZeroU16, 1),
            nz_unchecked!(NonZeroU16, 1),
            Cow::Borrowed(([0u8, 0]).as_ref()),
        );
        let bitmap = Bitmap::new(&info).unwrap();
        drop(bitmap);
    }
}
