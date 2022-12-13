/* -----------------------------------------------------------------------------------
 * src/draw.rs - Pens and brushes
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

//! Pens and brushes

use crate::mutexes::Mutex;
use core::{ptr::NonNull, sync::atomic::AtomicPtr};
use cty::c_int;
use winapi::{
    ctypes::c_void,
    shared::{
        minwindef::DWORD,
        windef::{HBRUSH__, HPEN__},
    },
    um::wingdi::{self, RGB},
};

/// The styles that a pen can have.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum PenStyle {
    Solid = wingdi::PS_SOLID,
    Dash = wingdi::PS_DASH,
    Dot = wingdi::PS_DOT,
    DashDot = wingdi::PS_DASHDOT,
    DashDotDot = wingdi::PS_DASHDOTDOT,
    Null = wingdi::PS_NULL,
    InsideFrame = wingdi::PS_INSIDEFRAME,
}

/// A pen that can be used to draw lines on the screen.
#[repr(transparent)]
pub struct Pen {
    hpen: Mutex<AtomicPtr<HPEN__>>,
}

impl Pen {
    /// Create a new pen from a color, line width, and style.
    #[inline]
    pub fn new(r: u8, g: u8, b: u8, width: u32, style: PenStyle) -> crate::Result<Self> {
        let crref = RGB(r, g, b);
        let hpen = unsafe { wingdi::CreatePen(style as DWORD as c_int, width as c_int, crref) };
        if hpen.is_null() {
            Err(crate::win32_error(crate::Win32Function::CreatePen))
        } else {
            Ok(Self {
                hpen: Mutex::new(AtomicPtr::new(hpen)),
            })
        }
    }

    /// Get the handle to this pen.
    ///
    /// # Safety
    ///
    /// This function copies the pointer out of an AtomicPtr and is thus unsound.
    #[inline]
    pub unsafe fn hpen(&self) -> NonNull<HPEN__> {
        let mut p = self.hpen.lock();
        let ptr = p.get_mut();
        debug_assert!(!ptr.is_null());
        NonNull::new_unchecked(*ptr)
    }
}

impl Drop for Pen {
    #[inline]
    fn drop(&mut self) {
        unsafe { wingdi::DeleteObject(*self.hpen.lock().get_mut() as *mut c_void) };
    }
}

/// A brush that can be used to paint onto the screen.
#[repr(transparent)]
pub struct Brush {
    hbrush: Mutex<AtomicPtr<HBRUSH__>>,
}

impl Brush {
    /// Create a new brush from a color.
    #[inline]
    pub fn solid(r: u8, g: u8, b: u8) -> crate::Result<Self> {
        let crref = RGB(r, g, b);
        let hbrush = unsafe { wingdi::CreateSolidBrush(crref) };
        if hbrush.is_null() {
            Err(crate::win32_error(crate::Win32Function::CreateBrush))
        } else {
            Ok(Self {
                hbrush: Mutex::new(AtomicPtr::new(hbrush)),
            })
        }
    }

    /// Get the handle to this brush.
    #[inline]
    pub unsafe fn hbrush(&self) -> NonNull<HBRUSH__> {
        let mut p = self.hbrush.lock();
        let ptr = p.get_mut();
        debug_assert!(!ptr.is_null());
        NonNull::new_unchecked(*ptr)
    }
}

impl Drop for Brush {
    #[inline]
    fn drop(&mut self) {
        unsafe { wingdi::DeleteObject(*self.hbrush.lock().get_mut() as *mut c_void) };
    }
}
