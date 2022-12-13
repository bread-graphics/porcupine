/* -----------------------------------------------------------------------------------
 * src/msg.rs - Functions wrapping around the message loop API.
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

// just re-export MSG
use core::{cmp::Ordering, ptr};
use maybe_uninit::MaybeUninit;
use winapi::um::winuser;
pub use winapi::um::winuser::MSG;

/// Get a message from the Win32 event loop.
#[inline]
pub fn get_message() -> crate::Result<Option<MSG>> {
    let mut m: MaybeUninit<MSG> = MaybeUninit::uninit();

    match unsafe { winuser::GetMessageA(m.as_mut_ptr(), ptr::null_mut(), 0, 0) }.cmp(&0) {
        Ordering::Greater => {
            // if GetMessage is greater than zero, we've created a valid message
            Ok(Some(unsafe { m.assume_init() }))
        }
        Ordering::Equal => {
            // if GetMessage is zero, this is a quit message
            Ok(None)
        }
        Ordering::Less => {
            // if GetMessage is less than zero, an error has occurred
            Err(crate::win32_error(crate::Win32Function::GetMessageA))
        }
    }
}

/// Translate the message from the Win32 event loop.
#[inline]
pub fn translate_message(m: &MSG) {
    // note: this function does not fail, according to the Win32 docs
    unsafe { winuser::TranslateMessage(m) };
}

/// Dispatch the message from the Win32 event loop.
#[inline]
pub fn dispatch_message(m: &MSG) {
    // note: the function returns the return value of the WndProc. This should be ignored.
    unsafe { winuser::DispatchMessageA(m) };
}
