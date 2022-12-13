/* -----------------------------------------------------------------------------------
 * src/bitmap.rs - A wrapper around the Win32 bitmap.
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

use crate::{mutexes::Mutex, DeviceContext};
use alloc::sync::{Arc, Weak};
use core::{mem, ptr::NonNull, sync::atomic::AtomicPtr};
use cty::{c_int, c_long};
use euclid::default::Size2D;
use maybe_uninit::MaybeUninit;
use winapi::{
    ctypes::c_void,
    shared::{minwindef::BYTE, windef::HBITMAP__},
    um::wingdi::{self, BITMAP},
};

static OWNING_DC_NONE: &'static str = "Owning DC was not properly set";

/// A bitmap.
pub struct Bitmap {
    hbitmap: Arc<Mutex<AtomicPtr<HBITMAP__>>>,
    owning_dc: Option<DeviceContext>, // Option only for initialization. This field should never be None
    bm: BITMAP,
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        // drop the owning dc before anything else
        mem::drop(self.owning_dc.take().expect(OWNING_DC_NONE));

        unsafe { wingdi::DeleteObject(*self.hbitmap.lock().get_mut() as *mut c_void) };
    }
}

impl Bitmap {
    /// Create a new bitmap from size and raw data. Data is expected to be raw RGB bytes.
    pub fn from_dc_and_data(
        dc: &DeviceContext,
        size: Size2D<c_int>,
        data: &[BYTE],
    ) -> crate::Result<Self> {
        let hbitmap = unsafe {
            wingdi::CreateBitmap(
                size.width,
                size.height,
                1,
                24,
                data.as_ptr() as *const c_void,
            )
        };

        if hbitmap.is_null() {
            Err(crate::win32_error(crate::Win32Function::CreateBitmap))
        } else {
            // basic bm
            let mut bm: MaybeUninit<BITMAP> = MaybeUninit::zeroed();
            if unsafe {
                wingdi::GetObjectA(
                    hbitmap as *mut c_void,
                    mem::size_of::<BITMAP>() as c_int,
                    bm.as_mut_ptr() as *mut c_void,
                )
            } == 0
            {
                return Err(crate::win32_error(crate::Win32Function::GetObjectA));
            }

            let mut b = Self {
                hbitmap: Arc::new(Mutex::new(AtomicPtr::new(hbitmap))),
                owning_dc: None,
                bm: unsafe { bm.assume_init() },
            };

            // set up a DC for drawing
            let mut owning_dc = dc.create_compatible()?; // TODO: this might cause a panic
            owning_dc.set_bitmap(&b)?; // TODO: same here

            b.owning_dc = Some(owning_dc);

            Ok(b)
        }
    }

    /// Get the handle to a bitmap.
    ///
    /// # Safety
    ///
    /// This copies the handle out of an AtomicPtr and is thus unsound.
    pub unsafe fn hbitmap(&self) -> NonNull<HBITMAP__> {
        let mut p = self.hbitmap.lock();
        let ptr = *p.get_mut();
        debug_assert!(!ptr.is_null());
        NonNull::new_unchecked(ptr)
    }

    /// Get the internal device context.
    pub fn dc(&self) -> &DeviceContext {
        self.owning_dc.as_ref().expect(OWNING_DC_NONE)
    }

    /// Get the width of the internal image.
    pub fn width(&self) -> c_long {
        self.bm.bmWidth
    }
    /// Get the height of the internal image.
    pub fn height(&self) -> c_long {
        self.bm.bmHeight
    }

    /// Get a weak reference to this bitmap.
    pub fn weak_reference(&self) -> Weak<Mutex<AtomicPtr<HBITMAP__>>> {
        Arc::downgrade(&self.hbitmap)
    }
}
