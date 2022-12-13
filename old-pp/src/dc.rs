/* -----------------------------------------------------------------------------------
 * src/dc.rs - A wrapper around the drawing context.
 * porcupine - Safe wrapper around the graphical parts of Win32.
 * Copyright © 2020 not_a_seagull
 *
 * This project is licensed under either the Apache 2.0 license or the MIT license, at
 * your option. For more information, please consult the LICENSE-APACHE or LICENSE-MIT
 * files in the repository root.
 * -----------------------------------------------------------------------------------
 * MIT License:
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the “Software”), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED “AS IS”, WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 * -----------------------------------------------------------------------------------
 * Apache 2.0 License Declaration:
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 * ----------------------------------------------------------------------------------
 */

use crate::{mutexes::Mutex, Bitmap, Brush, GenericWindow, Pen, WeakWindow};
use alloc::sync::Weak;
use core::{
    option::Option,
    ptr::{self, NonNull},
    sync::atomic::AtomicPtr,
};
use cty::c_int;
use euclid::default::{Point2D, Rect};
use maybe_uninit::MaybeUninit;
use winapi::{
    ctypes::c_void,
    shared::{
        minwindef::DWORD,
        windef::{HBITMAP__, HDC__},
    },
    um::{
        wingdi,
        winuser::{self, PAINTSTRUCT},
    },
};

/// The direction an arc can go in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub enum ArcDirection {
    Clockwise,
    CounterClockwise,
}

// GDI objects that can be stored in a device context
enum DeviceContextStorage {
    Bitmap(Weak<Mutex<AtomicPtr<HBITMAP__>>>),
}

// Types of device contexts that can be activated.
enum DeviceContextType {
    Painter {
        owner: WeakWindow,
        paint_struct: PAINTSTRUCT,
    },
    OwnsGDIObject {
        old_object: Option<Mutex<AtomicPtr<c_void>>>,
        storage: Option<DeviceContextStorage>,
    },
}

/// A drawing context.
pub struct DeviceContext {
    hdc: Mutex<AtomicPtr<HDC__>>,
    kind: DeviceContextType,
}

impl Drop for DeviceContext {
    #[allow(unused_variables)]
    fn drop(&mut self) {
        let mut hdc = self.hdc.lock();
        // we need to release differently depending on how we were allocated
        match self.kind {
            DeviceContextType::Painter {
                ref owner,
                ref paint_struct,
            } => {
                if let Some(o) = owner.hwnd.upgrade() {
                    let mut parent = o.lock();
                    // end the paint
                    unsafe {
                        winuser::EndPaint(*parent.get_mut(), paint_struct);
                    };
                }
            }
            DeviceContextType::OwnsGDIObject {
                ref old_object,
                ref storage,
            } => {
                if let Some(o) = old_object {
                    let mut l = o.lock();
                    unsafe { wingdi::SelectObject(*hdc.get_mut(), *l.get_mut()) };
                }

                unsafe { wingdi::DeleteDC(*hdc.get_mut()) };
            }
        }
    }
}

/// Operations for copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum CopyOperation {
    SrcCopy = wingdi::SRCCOPY,
    SrcAnd = wingdi::SRCAND,
    SrcErase = wingdi::SRCERASE,
    SrcPaint = wingdi::SRCPAINT,
}

impl DeviceContext {
    /// Start painting with a new DC.
    pub fn begin_paint<T: GenericWindow + ?Sized>(hwnd: &T) -> crate::Result<Self> {
        let mut ps: MaybeUninit<PAINTSTRUCT> = MaybeUninit::uninit();
        let hdc = unsafe { winuser::BeginPaint(hwnd.hwnd().as_mut(), ps.as_mut_ptr()) };

        if hdc.is_null() {
            Err(crate::win32_error(crate::Win32Function::BeginPaint))
        } else {
            Ok(Self {
                hdc: Mutex::new(AtomicPtr::new(hdc)),
                kind: DeviceContextType::Painter {
                    owner: hwnd.weak_reference(),
                    paint_struct: unsafe { ps.assume_init() },
                },
            })
        }
    }

    /// Create a compatible DC for another DC.
    pub fn create_compatible(&self) -> crate::Result<Self> {
        let hdc = unsafe { wingdi::CreateCompatibleDC(self.hdc().as_mut()) };

        if hdc.is_null() {
            Err(crate::win32_error(crate::Win32Function::CreateCompatibleDC))
        } else {
            Ok(Self {
                hdc: Mutex::new(AtomicPtr::new(hdc)),
                kind: DeviceContextType::OwnsGDIObject {
                    old_object: None,
                    storage: None,
                },
            })
        }
    }

    /// Set the pen for this DC.
    #[inline]
    pub fn set_pen(&self, pen: &Pen) {
        unsafe { wingdi::SelectObject(self.hdc().as_mut(), pen.hpen().as_ptr() as *mut c_void) };
    }

    /// Set the brush for this DC.
    #[inline]
    pub fn set_brush(&self, brush: &Brush) {
        unsafe {
            wingdi::SelectObject(self.hdc().as_mut(), brush.hbrush().as_ptr() as *mut c_void)
        };
    }

    /// Turn a compatible DC into a bitmap DC.
    pub fn set_bitmap(&mut self, bitmap: &Bitmap) -> crate::Result<()> {
        match self.kind {
            DeviceContextType::Painter { .. } => Err(crate::Error::NoGDIStorage),
            DeviceContextType::OwnsGDIObject {
                ref mut old_object,
                ref mut storage,
            } => {
                if old_object.is_some() || storage.is_some() {
                    return Err(crate::Error::AlreadyHadGDIStorage);
                }

                let old_ptr = unsafe {
                    wingdi::SelectObject(
                        *self.hdc.lock().get_mut(),
                        bitmap.hbitmap().as_ptr() as *mut c_void,
                    )
                };

                *old_object = Some(Mutex::new(AtomicPtr::new(old_ptr)));
                *storage = Some(DeviceContextStorage::Bitmap(bitmap.weak_reference()));

                Ok(())
            }
        }
    }

    /// Get a handle to this DC.
    ///
    /// # Safety
    ///
    /// This copies the pointer out of the AtomicPtr and is thus unsound.
    pub unsafe fn hdc(&self) -> NonNull<HDC__> {
        let mut p = self.hdc.lock();
        let ptr = *p.get_mut();
        debug_assert!(!ptr.is_null());
        NonNull::new_unchecked(ptr)
    }

    /// Copy data from one DC to another, using BitBlt.
    pub fn copy_from(
        &self,
        source: &Self,
        source_rect: Rect<c_int>,
        dest_pt: Point2D<c_int>,
        op: CopyOperation,
    ) -> crate::Result<()> {
        if unsafe {
            wingdi::BitBlt(
                self.hdc().as_mut(),
                dest_pt.x,
                dest_pt.y,
                source_rect.size.width,
                source_rect.size.height,
                source.hdc().as_mut(),
                source_rect.origin.x,
                source_rect.origin.y,
                op as DWORD,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::BitBlt))
        } else {
            Ok(())
        }
    }

    /// Move this DC to a coordinate point.
    pub fn move_to(&self, p: Point2D<c_int>) -> crate::Result<()> {
        if unsafe { wingdi::MoveToEx(self.hdc().as_mut(), p.x, p.y, ptr::null_mut()) } == 0 {
            Err(crate::win32_error(crate::Win32Function::MoveToEx))
        } else {
            Ok(())
        }
    }

    /// Draw a line between two points.
    pub fn draw_line(&self, p1: Point2D<c_int>, p2: Point2D<c_int>) -> crate::Result<()> {
        self.move_to(p1)?;
        if unsafe { wingdi::LineTo(self.hdc().as_mut(), p2.x, p2.y) } == 0 {
            Err(crate::win32_error(crate::Win32Function::LineTo))
        } else {
            Ok(())
        }
    }

    /// Draw an arc between two points, enclosed in a bounding rect.
    pub fn draw_arc(
        &self,
        bounds: Rect<c_int>,
        p1: Point2D<c_int>,
        p2: Point2D<c_int>,
    ) -> crate::Result<()> {
        if unsafe {
            wingdi::Arc(
                self.hdc().as_mut(),
                bounds.origin.x,
                bounds.origin.y,
                bounds.origin.x + bounds.size.width,
                bounds.origin.y + bounds.size.height,
                p1.x,
                p1.y,
                p2.x,
                p2.y,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::Arc))
        } else {
            Ok(())
        }
    }

    /// Set the arc direction of this item.
    #[inline]
    pub fn set_arc_direction(&self, dir: ArcDirection) -> crate::Result<()> {
        if unsafe {
            wingdi::SetArcDirection(
                self.hdc().as_mut(),
                match dir {
                    ArcDirection::Clockwise => wingdi::AD_CLOCKWISE,
                    ArcDirection::CounterClockwise => wingdi::AD_COUNTERCLOCKWISE,
                } as c_int,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::SetArcDirection))
        } else {
            Ok(())
        }
    }

    /// Draw a rectangle.
    #[inline]
    pub fn draw_rect(&self, rect: Rect<c_int>) -> crate::Result<()> {
        if unsafe {
            wingdi::Rectangle(
                self.hdc().as_mut(),
                rect.origin.x,
                rect.origin.y,
                rect.origin.x + rect.size.width,
                rect.origin.y + rect.size.height,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::Rectangle))
        } else {
            Ok(())
        }
    }

    /// Draw an ellipse.
    #[inline]
    pub fn draw_ellipse(&self, bounding_rect: Rect<c_int>) -> crate::Result<()> {
        if unsafe {
            wingdi::Ellipse(
                self.hdc().as_mut(),
                bounding_rect.origin.x,
                bounding_rect.origin.y,
                bounding_rect.origin.x + bounding_rect.size.width,
                bounding_rect.origin.y + bounding_rect.size.height,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::Ellipse))
        } else {
            Ok(())
        }
    }

    /// Set the brush color.
    pub fn set_brush_color(&self, r: u8, g: u8, b: u8) -> crate::Result<()> {
        let clr = wingdi::RGB(r, g, b);
        if unsafe { wingdi::SetDCBrushColor(self.hdc().as_mut(), clr) } == wingdi::CLR_INVALID {
            Err(crate::win32_error(crate::Win32Function::SetDCBrushColor))
        } else {
            Ok(())
        }
    }

    /// Set the pen color.
    pub fn set_pen_color(&self, r: u8, g: u8, b: u8) -> crate::Result<()> {
        let clr = wingdi::RGB(r, g, b);
        if unsafe { wingdi::SetDCBrushColor(self.hdc().as_mut(), clr) } == wingdi::CLR_INVALID {
            Err(crate::win32_error(crate::Win32Function::SetDCPenColor))
        } else {
            Ok(())
        }
    }
}
