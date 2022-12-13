/* -----------------------------------------------------------------------------------
 * src/error.rs - Common error type to keep things simple.
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

use alloc::{
    string::{FromUtf8Error, String, ToString},
    vec::Vec,
};
use core::{fmt, ptr};
use winapi::{
    shared::minwindef::DWORD,
    um::{errhandlingapi, winbase::*},
};

/// Win32 functions that are capable of erroring out.
#[derive(Debug, Clone, Copy, Hash)]
pub enum Win32Function {
    MultiByteToWideChar,
    WideCharToMultiByte,
    GetModuleHandleExA,
    UnregisterClassA,
    RegisterClassExA,
    GetClassInfoExA,
    CreateWindowExA,
    GetWindowPlacement,
    SetWindowPlacement,
    SetWindowTextA,
    InvalidateRect,
    MoveToEx,
    LineTo,
    SetDCBrushColor,
    SetDCPenColor,
    Arc,
    SetArcDirection,
    Rectangle,
    Ellipse,
    ShowWindow,
    UpdateWindow,
    CreateCompatibleBitmap,
    BeginPaint,
    CreateCompatibleDC,
    CreateBitmap,
    GetObjectA,
    BitBlt,
    InitCommonControlsEx,
    GetMessageA,
    SetWindowLongPtrA,
    GetWindowLongPtrA,
    ScreenToClient,
    GetCursorPos,
    CreatePen,
    CreateBrush,
    Other(&'static str),
}

impl fmt::Display for Win32Function {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match *self {
                Self::CreatePen => "CreatePen",
                Self::CreateBrush => "CreateBrush",
                Self::GetCursorPos => "GetCursorPos",
                Self::ScreenToClient => "ScreenToClient",
                Self::GetWindowLongPtrA => "GetWindowLongPtrA",
                Self::SetWindowLongPtrA => "SetWindowLongPtrA",
                Self::GetMessageA => "GetMessageA",
                Self::MultiByteToWideChar => "MultiByteToWideChar",
                Self::WideCharToMultiByte => "WideCharToMultiByte",
                Self::GetModuleHandleExA => "GetModuleHandleExA",
                Self::UnregisterClassA => "UnregisterClassA",
                Self::RegisterClassExA => "RegisterClassExA",
                Self::GetClassInfoExA => "GetClassInfoExA",
                Self::CreateWindowExA => "CreateWindowExA",
                Self::GetWindowPlacement => "GetWindowPlacement",
                Self::SetWindowPlacement => "SetWindowPlacement",
                Self::SetWindowTextA => "SetWindowTextA",
                Self::InvalidateRect => "InvalidateRect",
                Self::MoveToEx => "MoveToEx",
                Self::LineTo => "LineTo",
                Self::SetDCBrushColor => "SetDCBrushColor",
                Self::SetDCPenColor => "SetDCPenColor",
                Self::Arc => "Arc",
                Self::SetArcDirection => "SetArcDirection",
                Self::Rectangle => "Rectangle",
                Self::Ellipse => "Ellipse",
                Self::ShowWindow => "ShowWindow",
                Self::UpdateWindow => "UpdateWindow",
                Self::CreateCompatibleBitmap => "CreateCompatibleBitmap",
                Self::BeginPaint => "BeginPaint",
                Self::CreateCompatibleDC => "CreateCompatibleDC",
                Self::CreateBitmap => "CreateBitmap",
                Self::GetObjectA => "GetObjectA",
                Self::BitBlt => "BitBlt",
                Self::InitCommonControlsEx => "InitCommonControlsEx",
                Self::Other(s) => s,
            }
        )
    }
}

/// The error used by the Porcupine API.
#[derive(Debug, Clone)]
pub enum Error {
    Unreachable,
    StaticMsg(&'static str),
    /// A Win32 error occured.
    Win32 {
        code: DWORD,
        message: String,
        function: Win32Function,
    },
    Utf8(FromUtf8Error),
    /// Attempted to upgrade a dead Weak pointer.
    ExpiredWeakPtr,
    NoGDIStorage,
    AlreadyHadGDIStorage,
}

impl fmt::Display for Error {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Unreachable => f.pad("This error should not have been able to be reached"),
            Error::StaticMsg(s) => f.pad(s),
            Error::Win32 {
                code,
                message,
                function,
            } => write!(f, "{} threw error code {}: {}", function, code, message),
            Error::Utf8(u) => fmt::Display::fmt(u, f),
            Error::ExpiredWeakPtr => f.pad("Attempted to upgrade a dead Weak pointer"),
            Error::NoGDIStorage => f.pad("No GDI storage was found in the device context"),
            Error::AlreadyHadGDIStorage => {
                f.pad("GDI storage already exists within the device context")
            }
        }
    }
}

impl From<Error> for fmt::Error {
    fn from(_f: Error) -> Self {
        Self
    }
}

impl From<FromUtf8Error> for Error {
    fn from(futf8: FromUtf8Error) -> Self {
        Self::Utf8(futf8)
    }
}

/// A result, for conveinence.
pub type Result<T> = core::result::Result<T, Error>;

/// Get the last Win32 error, if applicable.
pub fn win32_error(function: Win32Function) -> Error {
    let error = unsafe { errhandlingapi::GetLastError() };

    const ERROR_BUFFER_SIZE: usize = 256;
    let mut error_buffer = Vec::with_capacity(ERROR_BUFFER_SIZE);

    let len = unsafe {
        FormatMessageA(
            FORMAT_MESSAGE_IGNORE_INSERTS
                | FORMAT_MESSAGE_FROM_SYSTEM
                | FORMAT_MESSAGE_ARGUMENT_ARRAY,
            ptr::null(),
            error,
            0,
            error_buffer.as_mut_ptr(),
            (ERROR_BUFFER_SIZE + 1) as DWORD,
            ptr::null_mut(),
        )
    };

    if len == 0 {
        return Error::Win32 {
            code: error,
            message: "No error message detected".to_string(),
            function,
        };
    }

    unsafe { error_buffer.set_len(len as usize) };

    match String::from_utf8(error_buffer.into_iter().map(|i| i as u8).collect()) {
        Ok(s) => Error::Win32 {
            code: error,
            message: s,
            function,
        },
        Err(e) => e.into(),
    }
}
