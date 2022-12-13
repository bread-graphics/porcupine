/* -----------------------------------------------------------------------------------
 * src/window.rs - A wrapper around a GUI window.
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
use alloc::{
    boxed::Box,
    format,
    string::String,
    sync::{Arc, Weak},
};
use core::{
    any::Any,
    convert::TryInto,
    fmt, mem,
    ptr::{self, NonNull},
    sync::atomic::AtomicPtr,
};
use cty::c_int;
use euclid::default::{Point2D, Rect};
use maybe_uninit::MaybeUninit;
use winapi::{
    ctypes::c_void,
    shared::{
        basetsd::LONG_PTR,
        minwindef::{DWORD, FALSE, TRUE, UINT},
        ntdef::LPCSTR,
        windef::{HBRUSH, HWND, HWND__, POINT},
    },
    um::{
        errhandlingapi,
        winuser::{
            self, COLOR_WINDOW, IDC_ARROW, IDI_APPLICATION, WINDOWPLACEMENT, WNDCLASSEXA, WNDPROC,
        },
    },
};

/// An owned, modifyable window class.
#[derive(Clone)]
pub struct OwnedWindowClass {
    inner: WNDCLASSEXA,
    is_registered: bool,
    class_name: String,
}

unsafe impl Send for OwnedWindowClass {}
unsafe impl Sync for OwnedWindowClass {}

impl fmt::Debug for OwnedWindowClass {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let name = format!("OwnedWindowClass (\"{}\")", self.class_name());
        f.debug_struct(&name)
            .field("inner", &format_args!("Window Class Structure")) // TODO: fix to use Debug trait
            .field("is_registered", &self.is_registered)
            .field("class_name", &self.class_name)
            .finish()
    }
}

impl OwnedWindowClass {
    /// Create a new WindowClass that has yet to be initialized.
    pub fn new(name: String) -> Self {
        // get the default icon
        let icon = unsafe { winuser::LoadIconW(ptr::null_mut(), IDI_APPLICATION) };

        // create the window class
        let inner = WNDCLASSEXA {
            cbSize: mem::size_of::<WNDCLASSEXA>() as UINT,
            lpfnWndProc: Some(winuser::DefWindowProcW),
            hInstance: unsafe { crate::MODULE_INFO.lock().handle().as_mut() },
            lpszClassName: name.as_ptr() as LPCSTR,
            hIcon: icon,
            hIconSm: icon,
            style: 0,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hCursor: unsafe { winuser::LoadCursorW(ptr::null_mut(), IDC_ARROW) },
            hbrBackground: unsafe {
                mem::transmute::<usize, HBRUSH>((COLOR_WINDOW + 1).try_into().unwrap())
            },
            lpszMenuName: ptr::null(),
        };

        Self {
            inner,
            is_registered: false,
            class_name: name,
        }
    }

    /// Get the name of the class.
    pub fn class_name(&self) -> &str {
        &self.class_name // guaranteed to be the class name
    }

    /// Set the name of the class.
    pub fn set_class_name(&mut self, name: String) -> crate::Result<()> {
        self.inner.lpszClassName = name.as_ptr() as LPCSTR;
        self.class_name = name; // make sure name isn't dropped

        Ok(())
    }

    /// Set the window procedure for the class.
    pub fn set_window_proc(&mut self, wndproc: WNDPROC) {
        self.inner.lpfnWndProc = wndproc;
    }

    /// Get the style for the window class.
    pub fn style(&self) -> UINT {
        self.inner.style
    }

    /// Set the style for the window class.
    pub fn set_style(&mut self, style: UINT) {
        self.inner.style = style;
    }

    /// Register this class. This function will unregister, then re-register the class
    /// if it is already registered.
    pub fn register(&mut self) -> crate::Result<()> {
        // if this is an already registered class, unregister it
        if self.is_registered {
            if unsafe {
                winuser::UnregisterClassA(
                    self.class_name.as_ptr() as LPCSTR,
                    crate::MODULE_INFO.lock().handle().as_mut(),
                )
            } == 0
            {
                return Err(crate::win32_error(crate::Win32Function::UnregisterClassA));
            } else {
                self.is_registered = false; // in the unlikely event of an error
            }
        }

        // register the class
        if unsafe { winuser::RegisterClassExA(&self.inner) } == 0 {
            Err(crate::win32_error(crate::Win32Function::RegisterClassExA))
        } else {
            self.is_registered = true;
            Ok(())
        }
    }
}

/// A window class; either a reference to a window class or a full, owned window class.
pub trait WindowClass {
    /// Convert this item into the name of the class.
    fn identifier(&self) -> &str;
}

impl WindowClass for OwnedWindowClass {
    fn identifier(&self) -> &str {
        self.class_name()
    }
}

impl WindowClass for String {
    fn identifier(&self) -> &str {
        &*self
    }
}

impl WindowClass for &str {
    fn identifier(&self) -> &str {
        self
    }
}

bitflags::bitflags! {
    pub struct WindowStyle : DWORD {
        const NONE = 0;
        const ACTIVE_CAPTION = winuser::WS_ACTIVECAPTION;
        const BORDER = winuser::WS_BORDER;
        const CAPTION = winuser::WS_CAPTION;
        const CHILD = winuser::WS_CHILD;
        const CHILD_WINDOW = winuser::WS_CHILDWINDOW;
        const CLIP_CHILDREN = winuser::WS_CLIPCHILDREN;
        const CLIP_SIBLINGS = winuser::WS_CLIPSIBLINGS;
        const DISABLED = winuser::WS_DISABLED;
        const DLG_FRAME = winuser::WS_DLGFRAME;
        const GROUP = winuser::WS_GROUP;
        const HSCROLL = winuser::WS_HSCROLL;
        const ICONIC = winuser::WS_ICONIC;
        const MAXIMIZE = winuser::WS_MAXIMIZE;
        const MAXIMIZE_BOX = winuser::WS_MAXIMIZEBOX;
        const MINIMIZE = winuser::WS_MINIMIZE;
        const MINIMIZE_BOX = winuser::WS_MINIMIZEBOX;
        const OVERLAPPED = winuser::WS_OVERLAPPED;
        const OVERLAPPED_WINDOW = winuser::WS_OVERLAPPEDWINDOW;
        const POPUP = winuser::WS_POPUP;
        const POPUP_WINDOW = winuser::WS_POPUPWINDOW;
        const SIZEBOX = winuser::WS_SIZEBOX;
        const SYSMENU = winuser::WS_SYSMENU;
        const TAB_STOP = winuser::WS_TABSTOP;
        const THICK_FRAME = winuser::WS_THICKFRAME;
        const TILED = winuser::WS_TILED;
        const TILED_WINDOW = winuser::WS_TILEDWINDOW;
        const VISIBLE = winuser::WS_VISIBLE;
        const VSCROLL = winuser::WS_VSCROLL;
    }
}

bitflags::bitflags! {
    pub struct ExtendedWindowStyle : DWORD {
        const NONE = 0;
        const ACCEPT_FILES = winuser::WS_EX_ACCEPTFILES;
        const APP_WINDOW = winuser::WS_EX_APPWINDOW;
        const CLIENT_EDGE = winuser::WS_EX_CLIENTEDGE;
        const COMPOSITED = winuser::WS_EX_COMPOSITED;
        const CONTEXT_HELP = winuser::WS_EX_CONTEXTHELP;
        const CONTROL_PARENT = winuser::WS_EX_CONTROLPARENT;
        const DLG_MODAL_FRAME = winuser::WS_EX_DLGMODALFRAME;
        const LAYERED = winuser::WS_EX_LAYERED;
        const LAYOUT_RTL = winuser::WS_EX_LAYOUTRTL;
        const LEFT = winuser::WS_EX_LEFT;
        const LEFT_SCROLL_BAR = winuser::WS_EX_LEFTSCROLLBAR;
        const LTR_READING = winuser::WS_EX_LTRREADING;
        const MDI_CHILD = winuser::WS_EX_MDICHILD;
        const NO_ACTIVATE = winuser::WS_EX_NOACTIVATE;
        const NO_INHERIT_LAYOUT = winuser::WS_EX_NOINHERITLAYOUT;
        const NO_PARENT_NOTIFY = winuser::WS_EX_NOPARENTNOTIFY;
        const NO_REDIRECTION_BITMAP = winuser::WS_EX_NOREDIRECTIONBITMAP;
        const OVERLAPPED_WINDOW = winuser::WS_EX_OVERLAPPEDWINDOW;
        const PALETTE_WINDOW = winuser::WS_EX_PALETTEWINDOW;
        const RIGHT = winuser::WS_EX_RIGHT;
        const RIGHT_SCROLL_BAR = winuser::WS_EX_RIGHTSCROLLBAR;
        const RTL_READING = winuser::WS_EX_RTLREADING;
        const STATIC_EDGE = winuser::WS_EX_STATICEDGE;
        const TOOL_WINDOW = winuser::WS_EX_TOOLWINDOW;
        const TOPMOST = winuser::WS_EX_TOPMOST;
        const TRANSPARENT = winuser::WS_EX_TRANSPARENT;
        const WINDOW_EDGE = winuser::WS_EX_WINDOWEDGE;
    }
}

/// Ways to show a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum CmdShow {
    Hide = winuser::SW_HIDE,
    ForceMinimize = winuser::SW_FORCEMINIMIZE,
    Maximize = winuser::SW_MAXIMIZE,
    Minimize = winuser::SW_MINIMIZE,
    Show = winuser::SW_SHOW,
    ShowDefault = winuser::SW_SHOWDEFAULT,
    ShowMinimized = winuser::SW_SHOWMINIMIZED,
    ShowNA = winuser::SW_SHOWNA,
    ShowNoActivate = winuser::SW_SHOWNOACTIVATE,
    ShowNormal = winuser::SW_SHOWNORMAL,
}

impl CmdShow {
    /// For consistency.
    #[allow(non_snake_case)]
    #[inline]
    pub fn ShowMaximize() -> Self {
        Self::Maximize
    }
}

/// A wrapper around the Win32 HWND.
pub struct Window {
    hwnd: Arc<Mutex<AtomicPtr<HWND__>>>,
    has_user_data: bool,
}

/// A weak wrapper around the Win32 HWND.
#[derive(Clone)]
#[repr(transparent)]
pub struct WeakWindow {
    pub(crate) hwnd: Weak<Mutex<AtomicPtr<HWND__>>>,
}

/// A reference to a window that doesn't run the standard drop procedures.
pub struct DroplessWindow {
    hwnd: Arc<Mutex<AtomicPtr<HWND__>>>,
}

/// A trait to generalize interactions with windows.
pub trait GenericWindow {
    /// Get the raw handle to this window.
    ///
    /// # Panics
    ///
    /// This function will panic if the internal mutex is unable to be locked, or if
    /// the window has already been dropped.
    fn hwnd(&self) -> NonNull<HWND__>;

    /// Create a weak reference to this window.
    fn weak_reference(&self) -> WeakWindow;

    /// Convert a point from the screen into coordinates relative to this window.
    #[inline]
    fn screen_to_client(&self, pt: Point2D<c_int>) -> crate::Result<Point2D<c_int>> {
        let mut lp = POINT {
            x: pt.x.into(),
            y: pt.y.into(),
        };
        if unsafe { winuser::ScreenToClient(self.hwnd().as_mut(), &mut lp) } == 0 {
            Err(crate::win32_error(crate::Win32Function::ScreenToClient))
        } else {
            Ok(Point2D::new(lp.x.into(), lp.y.into()))
        }
    }

    /// Change the bounds of this window.
    fn reshape(&self, rect: Rect<c_int>) -> crate::Result<()> {
        // create the window placement struct
        let mut wp: MaybeUninit<WINDOWPLACEMENT> = MaybeUninit::zeroed();
        let mut hwnd = self.hwnd();

        if unsafe { winuser::GetWindowPlacement(hwnd.as_mut(), wp.as_mut_ptr()) } == 0 {
            return Err(crate::win32_error(crate::Win32Function::GetWindowPlacement));
        }

        let mut wp = unsafe { wp.assume_init() };
        wp.rcNormalPosition.left = rect.origin.x;
        wp.rcNormalPosition.top = rect.origin.y;
        wp.rcNormalPosition.right = rect.origin.x + rect.size.width;
        wp.rcNormalPosition.bottom = rect.origin.y + rect.size.height;

        if unsafe { winuser::SetWindowPlacement(hwnd.as_mut(), &wp) } == 0 {
            Err(crate::win32_error(crate::Win32Function::SetWindowPlacement))
        } else {
            Ok(())
        }
    }

    /// Enable or unenable this window.
    #[inline]
    fn enable(&self, do_display: bool) {
        unsafe { winuser::EnableWindow(self.hwnd().as_mut(), crate::wboolify(do_display)) };
    }

    /// Show the window.
    #[inline]
    fn show(&self, cmd_show: CmdShow) {
        unsafe { winuser::ShowWindow(self.hwnd().as_mut(), cmd_show as c_int) };
    }

    /// Update the window.
    #[inline]
    fn update(&self) -> crate::Result<()> {
        if unsafe { winuser::UpdateWindow(self.hwnd().as_mut()) } == 0 {
            Err(crate::win32_error(crate::Win32Function::UpdateWindow))
        } else {
            Ok(())
        }
    }

    /// Set the text value of this window.
    #[inline]
    fn set_text(&self, text: &str) -> crate::Result<()> {
        // note: i've personally tested this in C. You can delete the actual
        // allocated memory if you've already run SetWindowText.
        if unsafe { winuser::SetWindowTextA(self.hwnd().as_mut(), text.as_ptr() as LPCSTR) } == 0 {
            Err(crate::win32_error(crate::Win32Function::SetWindowTextA))
        } else {
            Ok(())
        }
    }

    /// Invalid this window and force a redraw.
    #[inline]
    fn invalidate(&self, invalidated_rect: Option<Rect<c_int>>) -> crate::Result<()> {
        let rect = invalidated_rect.map(crate::eurect_to_winrect);
        if unsafe {
            winuser::InvalidateRect(
                self.hwnd().as_mut(),
                match rect {
                    Some(ref r) => r,
                    None => ptr::null(),
                },
                TRUE,
            )
        } == 0
        {
            Err(crate::win32_error(crate::Win32Function::InvalidateRect))
        } else {
            Ok(())
        }
    }

    /// Send a paint event to this window.
    #[inline]
    fn repaint(&self) {
        unsafe { winuser::SendMessageA(self.hwnd().as_mut(), winuser::WM_PAINT, 0, 0) };
    }

    /// Begin painting ops on this window.
    #[inline]
    fn begin_paint(&self) -> crate::Result<DeviceContext> {
        DeviceContext::begin_paint(self)
    }
}

#[inline]
unsafe fn get_hwnd(a: &Arc<Mutex<AtomicPtr<HWND__>>>) -> NonNull<HWND__> {
    let mut p = a.lock();
    let ptr = p.get_mut();
    debug_assert!(!ptr.is_null());
    NonNull::new_unchecked(*ptr)
}

impl GenericWindow for Window {
    fn hwnd(&self) -> NonNull<HWND__> {
        // TODO: this might be unsound. check it later.
        unsafe { get_hwnd(&self.hwnd) }
    }

    fn weak_reference(&self) -> WeakWindow {
        WeakWindow {
            hwnd: Arc::downgrade(&self.hwnd),
        }
    }
}

impl GenericWindow for DroplessWindow {
    fn hwnd(&self) -> NonNull<HWND__> {
        unsafe { get_hwnd(&self.hwnd) }
    }

    fn weak_reference(&self) -> WeakWindow {
        WeakWindow {
            hwnd: Arc::downgrade(&self.hwnd),
        }
    }
}

impl GenericWindow for WeakWindow {
    fn hwnd(&self) -> NonNull<HWND__> {
        let upgraded = self
            .hwnd
            .upgrade()
            .expect("Unable to upgrade weak window into strong window.");

        unsafe { get_hwnd(&upgraded) }
    }

    fn weak_reference(&self) -> WeakWindow {
        self.clone()
    }
}

impl Window {
    /// Create a new window with a specified creation parameter.
    pub fn with_creation_param<WC: WindowClass, T: Any>(
        window_class: &WC,
        window_name: &str,
        style: WindowStyle,
        extended_style: ExtendedWindowStyle,
        bounds: Rect<c_int>,
        parent: Option<&Self>,
        create_parameter: Option<Box<T>>,
    ) -> crate::Result<Self> {
        let parent = match parent {
            Some(p) => unsafe { p.hwnd().as_mut() },
            None => ptr::null_mut(),
        };

        let lpparam = match create_parameter {
            Some(c) => Box::into_raw(c),
            None => ptr::null_mut(),
        };

        let hwnd = unsafe {
            winuser::CreateWindowExA(
                extended_style.bits(),
                window_class.identifier().as_ptr() as LPCSTR,
                window_name.as_ptr() as LPCSTR,
                style.bits(),
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
                parent,
                ptr::null_mut(),
                crate::MODULE_INFO.lock().handle().as_mut(),
                lpparam as *mut c_void,
            )
        };

        if hwnd.is_null() {
            Err(crate::win32_error(crate::Win32Function::CreateWindowExA))
        } else {
            Ok(Self {
                hwnd: Arc::new(Mutex::new(AtomicPtr::new(hwnd))),
                has_user_data: false,
            })
        }
    }

    /// Create a new window.
    #[inline]
    pub fn new<WC: WindowClass>(
        window_class: &WC,
        window_name: &str,
        style: WindowStyle,
        extended_style: ExtendedWindowStyle,
        bounds: Rect<c_int>,
        parent: Option<&Self>,
    ) -> crate::Result<Self> {
        Self::with_creation_param::<WC, ()>(
            window_class,
            window_name,
            style,
            extended_style,
            bounds,
            parent,
            None,
        )
    }

    /// Set the user data field of this window to a pointer.
    ///
    /// Note: This does not set has_user_data because we don't know the nature of the pointer.
    #[inline]
    pub unsafe fn set_user_data_pointer<T: ?Sized>(&self, ptr: *mut T) -> crate::Result<()> {
        errhandlingapi::SetLastError(0);

        if winuser::SetWindowLongPtrA(
            self.hwnd().as_mut(),
            winuser::GWLP_USERDATA,
            ptr as *const () as LONG_PTR,
        ) == FALSE as LONG_PTR
            && errhandlingapi::GetLastError() != 0
        {
            Err(crate::win32_error(crate::Win32Function::SetWindowLongPtrA))
        } else {
            Ok(())
        }
    }

    /// Set the user data of this window to a box.
    #[inline]
    pub fn set_user_data_box<T: ?Sized>(&mut self, b: Box<T>) -> crate::Result<()> {
        unsafe { self.set_user_data_pointer(Box::into_raw(b)) }?;
        self.has_user_data = true;
        Ok(())
    }

    /// Get the user data of this window.
    #[inline]
    pub fn user_data<T: Any>(&self) -> crate::Result<&T> {
        let res =
            unsafe { winuser::GetWindowLongPtrA(self.hwnd().as_mut(), winuser::GWLP_USERDATA) };

        if res == FALSE as LONG_PTR {
            return Err(crate::win32_error(crate::Win32Function::GetWindowLongPtrA));
        }

        let res = unsafe { mem::transmute::<LONG_PTR, *const T>(res) };
        Ok(unsafe { &*res })
    }

    /// Take the user data of this window out.
    #[inline]
    pub fn take_user_data<T: Any>(&mut self) -> crate::Result<Box<T>> {
        unsafe { errhandlingapi::SetLastError(0) };

        let res = unsafe {
            winuser::SetWindowLongPtrA(
                self.hwnd().as_mut(),
                winuser::GWLP_USERDATA,
                ptr::null_mut() as *const () as LONG_PTR,
            )
        };
        self.has_user_data = false;

        if res == FALSE as LONG_PTR && unsafe { errhandlingapi::GetLastError() } != 0 {
            return Err(crate::win32_error(crate::Win32Function::SetWindowLongPtrA));
        }

        let res: Box<dyn Any + 'static> =
            unsafe { Box::from_raw(mem::transmute::<LONG_PTR, *mut ()>(res) as *mut dyn Any) };
        // downcast to T
        Box::<dyn Any + 'static>::downcast::<T>(res)
            .map_err(|_| crate::Error::StaticMsg("Unable to downcast user data"))
    }
}

impl DroplessWindow {
    /// Create a new dropless window.
    ///
    /// # Safety
    ///
    /// This function is unsafe because it doesn't follow the usual safety rules for Window.
    pub unsafe fn new(hwnd: HWND) -> DroplessWindow {
        Self {
            hwnd: Arc::new(Mutex::new(AtomicPtr::new(hwnd))),
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // if we have user data, dispose of it
        if self.has_user_data {
            let pointer = unsafe {
                mem::transmute::<LONG_PTR, *mut ()>(winuser::GetWindowLongPtrA(
                    self.hwnd().as_mut(),
                    winuser::GWLP_USERDATA,
                )) as *mut dyn Any
            };
            let _b = unsafe { Box::from_raw(pointer) }; // drops the box
        }
    }
}
