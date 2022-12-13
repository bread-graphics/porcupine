// Boost/Apache2 License

//! Functions for managing device contexts.

use crate::bitmap::Bitmap;
use crate::gdi_object::OwnedGdiObject;
use crate::region::Region;
use crate::window::{BorrowedWindow, GetDcFlags, RegionType};
use crate::Error;
use __sealed::Sealed;
use blood_geometry::{Point, Rect, Size};

use core::cell::Cell;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, EndPaint, GetDCEx,
    ReleaseDC, SetPixel, StretchBlt, SelectObject, MoveToEx
};
use windows_sys::Win32::Graphics::Gdi::{
    BLACKNESS, CAPTUREBLT, DCX_EXCLUDERGN, DCX_INTERSECTRGN, DSTINVERT, MERGECOPY, MERGEPAINT,
    NOTSRCCOPY, NOTSRCERASE, PATCOPY, PATINVERT, PATPAINT, SRCAND, SRCCOPY, SRCERASE, SRCINVERT,
    SRCPAINT, WHITENESS,
};
use windows_sys::Win32::Graphics::Gdi::{HDC, PAINTSTRUCT};

use windows_sys::Win32::Foundation::HWND;

/// A device context.
pub struct DeviceContext<Releaser: ReleaseDC + ?Sized> {
    /// The device context.
    handle: HDC,

    /// This handle is `Send` but `!Sync`.
    _thread_safety: PhantomData<Cell<()>>,

    /// The releaser for the device context.
    releaser: Releaser,
}

impl<Releaser: ReleaseDC + ?Sized> Drop for DeviceContext<Releaser> {
    fn drop(&mut self) {
        unsafe {
            self.releaser.release_dc(self.handle);
        }
    }
}

/// A DC Releaser corresponding to the `BeginPaint` and `EndPaint` syscalls.
pub struct PaintReleaser<'a> {
    _marker: PhantomData<&'a mut ()>,
}

unsafe impl Sealed for PaintReleaser<'_> {
    unsafe fn release_dc(&mut self, dc: HDC) {
        // Do nothing, it's freed for us later.
    }
}

unsafe impl ReleaseDC for PaintReleaser<'_> {}

/// A DC Releaser corresponding to the `GetDCEx` and `ReleaseDC` syscalls.
pub struct GetReleaser<'a> {
    window: Option<BorrowedWindow<'a>>,
}

unsafe impl Sealed for GetReleaser<'_> {
    unsafe fn release_dc(&mut self, dc: HDC) {
        ReleaseDC(self.window.map_or(0, |w| w.handle()), dc);
    }
}

unsafe impl ReleaseDC for GetReleaser<'_> {}

/// A DC Releaser corresponding to the `DeleteDC` syscall.
pub struct DeleteReleaser {
    _marker: PhantomData<()>,
}

unsafe impl Sealed for DeleteReleaser {
    unsafe fn release_dc(&mut self, dc: HDC) {
        DeleteDC(dc);
    }
}

unsafe impl ReleaseDC for DeleteReleaser {}

impl<'a> DeviceContext<PaintReleaser<'a>> {
    /// Begin painting a window.
    pub(crate) fn begin_paint<R>(
        window: BorrowedWindow<'_>,
        f: impl FnOnce(&mut Self, &mut PAINTSTRUCT) -> Result<R, Error>,
    ) -> Result<R, Error> {
        // Create a PAINTSTRUCT and then call BeginPaint.
        let mut ps = MaybeUninit::uninit();
        let dc = unsafe { BeginPaint(window.handle(), ps.as_mut_ptr()) };

        // If BeginPaint failed, return an error.
        if dc == 0 {
            return Err(Error::last_error("BeginPaint"));
        }

        /// Guard that calls EndPaint on drop.
        ///
        /// This ensures that EndPaint is called, even in a panic.
        struct PaintEnder<'a, 'b> {
            window: BorrowedWindow<'a>,
            ps: &'b mut PAINTSTRUCT,
        }

        impl Drop for PaintEnder<'_, '_> {
            fn drop(&mut self) {
                unsafe {
                    EndPaint(self.window.handle(), self.ps);
                }
            }
        }

        // Run the function.
        let guard = PaintEnder {
            window,
            ps: unsafe { &mut *ps.as_mut_ptr() },
        };
        let mut dc = Self {
            handle: dc,
            _thread_safety: PhantomData,
            releaser: PaintReleaser {
                _marker: PhantomData,
            },
        };

        f(&mut dc, &mut *guard.ps)
    }
}

impl<'a> DeviceContext<GetReleaser<'a>> {
    pub(crate) fn get_dc(
        window: Option<BorrowedWindow<'a>>,
        region: RegionType,
        flags: GetDcFlags,
    ) -> Result<Self, Error> {
        let mut flags = flags.bits();

        let region = match region {
            RegionType::None => 0,
            RegionType::Intersect(region) => {
                flags |= DCX_INTERSECTRGN;
                region.into_handle()
            }
            RegionType::Exclude(region) => {
                flags |= DCX_EXCLUDERGN;
                region.into_handle()
            }
        };

        let dc = unsafe { GetDCEx(window.map_or(0, |w| w.handle()), region, flags) };

        // If GetDCEx failed, return an error.
        if dc == 0 {
            Err(Error::last_error("GetDCEx"))
        } else {
            Ok(Self {
                handle: dc,
                _thread_safety: PhantomData,
                releaser: GetReleaser { window },
            })
        }
    }
}

impl<Releaser: ReleaseDC + ?Sized> DeviceContext<Releaser> {
    /// Create a compatible device context with this one.
    pub fn create_compatible_dc(&self) -> Result<DeviceContext<DeleteReleaser>, Error> {
        let dc = unsafe { CreateCompatibleDC(self.handle) };

        // If CreateCompatibleDC failed, return an error.
        if dc == 0 {
            Err(Error::last_error("CreateCompatibleDC"))
        } else {
            Ok(DeviceContext {
                handle: dc,
                _thread_safety: PhantomData,
                releaser: DeleteReleaser {
                    _marker: PhantomData,
                },
            })
        }
    }

    /// Create a bitmap compatible with this device context.
    pub fn create_compatible_bitmap(&self, size: Size<i32>) -> Result<Bitmap, Error> {
        let [width, height]: [i32; 2] = size.into();
        let bitmap = unsafe { CreateCompatibleBitmap(self.handle, width, height) };

        // If CreateCompatibleBitmap failed, return an error.
        if bitmap == 0 {
            Err(Error::last_error("CreateCompatibleBitmap"))
        } else {
            Ok(Bitmap::from(unsafe { OwnedGdiObject::new(bitmap) }))
        }
    }

    /// Select a GDI object into this device context.
    pub fn select_object(
        &self,
        object: impl Into<OwnedGdiObject>,
    ) -> Result<OwnedGdiObject, Error> {
        let old_object = unsafe { SelectObject(self.handle, object.into().into_handle()) };

        // If SelectObject failed, return an error.
        if old_object == 0 {
            Err(Error::last_error("SelectObject"))
        } else {
            Ok(unsafe { OwnedGdiObject::new(old_object) })
        }
    }

    /// Preform a bit-block color transfer from one DC to another.
    pub fn bit_blt(
        &self,
        src: &DeviceContext<impl ReleaseDC + ?Sized>,
        dest_rect: Rect<i32>,
        src_point: Point<i32>,
        op: BitBltOp,
    ) -> Result<(), Error> {
        let [x, y]: [i32; 2] = dest_rect.origin().into();
        let [width, height]: [i32; 2] = dest_rect.size().into();
        let [x_src, y_src]: [i32; 2] = src_point.into();

        let result = unsafe {
            BitBlt(
                self.handle,
                x,
                y,
                width,
                height,
                src.handle,
                x_src,
                y_src,
                op as _,
            )
        };

        // If BitBlt failed, return an error.
        if result == 0 {
            Err(Error::last_error("BitBlt"))
        } else {
            Ok(())
        }
    }

    /// Preform a bit-block transfer but resize the source to fit the destination.
    pub fn stretch_blt(
        &self,
        src: &DeviceContext<impl ReleaseDC + ?Sized>,
        dest_rect: Rect<i32>,
        src_rect: Rect<i32>,
        op: BitBltOp,
    ) -> Result<(), Error> {
        let [x, y]: [i32; 2] = dest_rect.origin().into();
        let [width, height]: [i32; 2] = dest_rect.size().into();
        let [x_src, y_src]: [i32; 2] = src_rect.origin().into();
        let [width_src, height_src]: [i32; 2] = src_rect.size().into();

        let result = unsafe {
            StretchBlt(
                self.handle,
                x,
                y,
                width,
                height,
                src.handle,
                x_src,
                y_src,
                width_src,
                height_src,
                op as _,
            )
        };

        // If StretchBlt failed, return an error.
        if result == 0 {
            Err(Error::last_error("StretchBlt"))
        } else {
            Ok(())
        }
    }

    /// Moves the DC origin to the specified point.
    pub fn move_to(&self, point: Point<i32>) -> Result<(), Error> {
        let [x, y]: [i32; 2] = point.into();
        let result = unsafe { MoveToEx(self.handle, x, y, 0 as _) };

        // If MoveToEx failed, return an error.
        if result == 0 {
            Err(Error::last_error("MoveToEx"))
        } else {
            Ok(())
        }
    }

    /// Set a pixel in the device context.
    pub fn set_pixel(&self, point: Point<i32>, color: u32) -> Result<(), Error> {
        let [x, y]: [i32; 2] = point.into();
        let result = unsafe { SetPixel(self.handle, x, y, color) };

        // If SetPixel failed, return an error.
        if result == 0 {
            Err(Error::last_error("SetPixel"))
        } else {
            Ok(())
        }
    }
}

/// Operations for bit-block device transfer.
#[repr(u32)]
pub enum BitBltOp {
    /// Fill the destination rectangle using the color associated with index 0 in the physical palette.
    Blackness = BLACKNESS,

    /// Include any windows that overlap the destination rectangle.
    CaptureBlt = CAPTUREBLT,

    /// Inverts the destination rectangle.
    DstInverse = DSTINVERT,

    /// Merges the colors of the source rectangle with the brush currently selected in hdcDest, by
    /// using the Boolean AND operator.
    MergeCopy = MERGECOPY,

    /// Merges the colors of the inverted source rectangle with the colors of the destination
    /// rectangle by using the Boolean OR operator.
    MergePaint = MERGEPAINT,

    /// Copies the inverted source rectangle to the destination.
    NotSrcCopy = NOTSRCCOPY,

    /// Combines the colors of the source and destination rectangles by using the Boolean OR operator.
    NotSrcErase = NOTSRCERASE,

    /// Copies the brush currently selected in hdcDest, to the destination bitmap.
    PatCopy = PATCOPY,

    /// Combines the colors of the brush currently selected in hdcDest, with the colors of the
    /// destination rectangle by using the Boolean XOR operator.
    PatInvert = PATINVERT,

    /// Combines the colors of the brush currently selected in hdcDest, with the colors of the
    /// inverted source rectangle by using the Boolean OR operator.
    PatPaint = PATPAINT,

    /// Combines the colors of the source and destination rectangles by using the Boolean AND
    /// operator.
    SrcAnd = SRCAND,

    /// Copies the source rectangle directly to the destination rectangle.
    SrcCopy = SRCCOPY,

    /// Combines the inverted colors of the destination rectangle with the colors of the source
    /// rectangle by using the Boolean AND operator.
    SrcErase = SRCERASE,

    /// Combines the colors of the source and destination rectangles by using the Boolean XOR
    /// operator.
    SrcInvert = SRCINVERT,

    /// Combines the colors of the source and destination rectangles by using the Boolean OR
    /// operator.
    SrcPaint = SRCPAINT,

    /// Fills the destination rectangle using the color associated with index 1 in the physical palette.
    ///
    /// This color is white for the default physical palette.
    Whiteness = WHITENESS,
}

/// The releaser for a device context.
///
/// # Safety
///
/// This trait should not be implemented outside of this crate.
pub unsafe trait ReleaseDC: Sealed {}

mod __sealed {
    use super::*;

    #[doc(hidden)]
    pub unsafe trait Sealed {
        /// Release the device context.
        unsafe fn release_dc(&mut self, dc: HDC);
    }
}
