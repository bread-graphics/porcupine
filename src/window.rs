// Boost/Apache2 License

use crate::class::{ClassData, ErasedClassData, WindowClass};
use crate::client::Client;
use crate::cstr::CStr;
use crate::dc::{DeviceContext, GetReleaser};
use crate::event::Event;
use crate::menu::Menu;
use crate::module::current_module;
use crate::region::Region;
use crate::{strict, Error};

use blood_geometry::{Point, Rect, Size};

use alloc::collections::VecDeque;
use alloc::rc::Rc;

use core::any::Any;
use core::cell::{Cell, RefCell};
use core::convert::Infallible;
use core::fmt;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::num::NonZeroU32;
use core::ptr;

use windows_sys::Win32::Foundation::{HWND, RECT};

use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, InvalidateRect, ScreenToClient};
use windows_sys::Win32::Graphics::Gdi::{
    DCX_CACHE, DCX_CLIPCHILDREN, DCX_CLIPSIBLINGS, DCX_LOCKWINDOWUPDATE, DCX_PARENTCLIP, DCX_WINDOW,
};

use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DestroyWindow, GetClientRect, GetDesktopWindow, GetWindowLongPtrA,
    GetWindowRect, SetWindowPos, SetWindowTextA, ShowWindow,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWLP_USERDATA, HWND_BOTTOM, HWND_NOTOPMOST, HWND_TOP, HWND_TOPMOST, SWP_DEFERERASE,
    SWP_DRAWFRAME, SWP_FRAMECHANGED, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOCOPYBITS, SWP_NOMOVE,
    SWP_NOOWNERZORDER, SWP_NOREDRAW, SWP_NOREPOSITION, SWP_NOSENDCHANGING, SWP_NOSIZE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_FORCEMINIMIZE, SW_HIDE, SW_MINIMIZE, SW_NORMAL, SW_SHOW,
    SW_SHOWDEFAULT, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, SW_SHOWNA,
    SW_SHOWNOACTIVATE, SW_SHOWNORMAL, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_DISABLED, WS_DLGFRAME, WS_EX_ACCEPTFILES, WS_EX_APPWINDOW,
    WS_EX_CLIENTEDGE, WS_EX_COMPOSITED, WS_EX_CONTEXTHELP, WS_EX_CONTROLPARENT,
    WS_EX_DLGMODALFRAME, WS_EX_LAYERED, WS_EX_LAYOUTRTL, WS_EX_LEFT, WS_EX_LEFTSCROLLBAR,
    WS_EX_MDICHILD, WS_EX_NOACTIVATE, WS_EX_NOINHERITLAYOUT, WS_EX_NOPARENTNOTIFY,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_OVERLAPPEDWINDOW, WS_EX_PALETTEWINDOW, WS_EX_RIGHT,
    WS_EX_RIGHTSCROLLBAR, WS_EX_RTLREADING, WS_EX_STATICEDGE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_EX_WINDOWEDGE, WS_GROUP, WS_HSCROLL, WS_MAXIMIZE, WS_MAXIMIZEBOX,
    WS_MINIMIZE, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_POPUPWINDOW,
    WS_SIZEBOX, WS_TABSTOP, WS_THICKFRAME, WS_VISIBLE, WS_VSCROLL,
};

impl Client {
    /// Get the top-level window.
    pub fn desktop_window(&self) -> BorrowedWindow<'static> {
        unsafe { BorrowedWindow::from_raw_handle(GetDesktopWindow()) }
    }

    /// Create a new window.
    pub fn create_window<'a, T>(
        &self,
        class: &WindowClass<'a, T>,
        title: &'a CStr,
        menu: Option<Menu>,
        parent: Option<BorrowedWindow<'_>>,
        style: WindowStyle,
        extended_style: ExtendedStyle,
        rectangle: Rect<i32>,
        window_data: T,
    ) -> Result<Window<'a, T>, Error> {
        // Box the window data to pass it in.
        let window_data = Box::into_raw(Box::new(window_data));
        assert!(!window_data.is_null());

        // Create the window.
        let hwnd = unsafe {
            CreateWindowExA(
                extended_style.bits(),
                class.ptr(),
                title.as_ptr().cast(),
                style.bits(),
                rectangle.origin().x(),
                rectangle.origin().y(),
                rectangle.size().width(),
                rectangle.size().height(),
                parent.map_or(0, |p| p.hwnd),
                menu.map_or(0, |m| m.into_handle()),
                current_module(),
                window_data as *mut _ as *const _,
            )
        };

        // Check for errors.
        if hwnd == 0 {
            // Free the window data.
            unsafe {
                drop(Box::from_raw(window_data));
            }

            return Err(Error::last_error("CreateWindowEx"));
        }

        // Bump the window count.
        self.increment_window_count();

        // Return the window.
        let window = Window {
            hwnd,
            _window_class: PhantomData,
            _window_data: PhantomData,
            _thread_unsafe: PhantomData,
        };

        // If a panic happened during window creation, we need to propagate it.
        window.as_window().propagate_panic();

        Ok(window)
    }
}

/// A window owned by the current context.
pub struct Window<'er, T> {
    /// The window handle.
    hwnd: HWND,

    /// References to the window class and its data.
    ///
    /// Logically, this is stored inside of the `HWND`.
    _window_class: PhantomData<*const WindowClass<'er, T>>,

    /// The window's window-specific data.
    ///
    /// Logically, this is stored inside of the `HWND`.
    _window_data: PhantomData<T>,

    /// We are not `Send` or `Sync`.
    _thread_unsafe: PhantomData<*mut ()>,
}

impl<T: fmt::Debug> fmt::Debug for Window<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct HexDebug(HWND);

        impl fmt::Debug for HexDebug {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:#010x}", self.0)
            }
        }

        // Get a pointer to the user data.
        let index = unsafe { GetWindowLongPtrA(self.hwnd, GWLP_USERDATA) };
        let user_data = strict::reconstitute(index).cast::<T>();

        f.debug_struct("Window")
            .field("hwnd", &HexDebug(self.hwnd))
            .field("user_data", unsafe { &*user_data })
            .finish()
    }
}

impl<'a, T> Drop for Window<'a, T> {
    fn drop(&mut self) {
        // Destroy the window, and the window proc will take care of the rest.
        unsafe {
            DestroyWindow(self.hwnd);
        }
    }
}

/// A borrowed window.
#[derive(Copy, Clone)]
pub struct BorrowedWindow<'a> {
    /// The window handle.
    hwnd: HWND,

    /// Eat the lifetime.
    _marker: PhantomData<&'a Window<'a, Infallible>>,
}

impl<'a> BorrowedWindow<'a> {
    /// Create a `BorrowedWindow` from a `HWND`.
    ///
    /// # Safety
    ///
    /// The HWND must be a valid window handle.
    pub unsafe fn from_raw_handle(hwnd: HWND) -> Self {
        Self {
            hwnd,
            _marker: PhantomData,
        }
    }

    pub(crate) fn handle(&self) -> HWND {
        self.hwnd
    }

    /// Propagate a panic, if one exists.
    fn propagate_panic(&self) {
        // Get the window data.
        let index = unsafe { GetWindowLongPtrA(self.hwnd, GWLP_USERDATA) };
        let user_data = strict::reconstitute(index).cast::<fn(*const ())>();

        // Call the panic handler.
        unsafe {
            (*user_data)(user_data.cast());
        }
    }
}

impl fmt::Debug for BorrowedWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct HexDisplay(HWND);

        impl fmt::Debug for HexDisplay {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{:#010x}", self.0)
            }
        }

        f.debug_tuple("BorrowedWindow")
            .field(&HexDisplay(self.hwnd))
            .finish()
    }
}

/// Something that can be represented as a window.
pub trait AsWindow {
    /// Get the window handle.
    fn as_window(&self) -> BorrowedWindow<'_>;

    /// Submit a display command to the window.
    fn show(&self, command: ShowCommand) {
        unsafe {
            ShowWindow(self.as_window().hwnd, command.bits());
        }
    }

    /// Set the title of the window.
    fn set_title(&self, title: &CStr) -> Result<(), Error> {
        let result = unsafe { SetWindowTextA(self.as_window().hwnd, title.as_ptr().cast()) };

        if result == 0 {
            Err(Error::last_error("SetWindowText"))
        } else {
            Ok(())
        }
    }

    /// Get the rectangle for the client area of the window.
    fn client_rect(&self) -> Result<Rect<i32>, Error> {
        unsafe {
            // The blood geometry rectangle and RECT have the same layout.
            let mut rect = MaybeUninit::<Rect<i32>>::zeroed();
            let result = GetClientRect(self.as_window().hwnd, &mut rect as *mut _ as *mut _);

            // Check for errors.
            if result == 0 {
                Err(Error::last_error("GetClientRect"))
            } else {
                Ok(rect.assume_init())
            }
        }
    }

    /// Get the rectangle for the window.
    fn window_rect(&self) -> Rect<i32> {
        unsafe {
            // The blood geometry rectangle and RECT have the same layout.
            let mut rect = MaybeUninit::<Rect<i32>>::zeroed();
            GetWindowRect(self.as_window().hwnd, &mut rect as *mut _ as *mut _);
            rect.assume_init()
        }
    }

    /// Invalidate the window.
    fn invalidate(&self, rect: Option<Rect<i32>>, erase: bool) -> Result<(), Error> {
        let result = unsafe {
            InvalidateRect(
                self.as_window().hwnd,
                rect.as_ref()
                    .map(|r| r as *const _ as *const _)
                    .unwrap_or(ptr::null()),
                erase as _,
            )
        };

        if result == 0 {
            Err(Error::last_error("InvalidateRect"))
        } else {
            Ok(())
        }
    }

    /// Set the window's position.
    fn set_window_pos(
        &self,
        insert_after: Option<InsertAfter<'_>>,
        position: Option<Point<i32>>,
        size: Option<Size<i32>>,
        flags: WindowPosFlags,
    ) -> Result<(), Error> {
        let mut flags = flags.bits();

        // Determine the insert after field/flag.
        let insert_after = match insert_after {
            Some(InsertAfter::Window(hwnd)) => hwnd.hwnd,
            Some(InsertAfter::Bottom) => HWND_BOTTOM,
            Some(InsertAfter::Top) => HWND_TOP,
            Some(InsertAfter::TopMost) => HWND_TOPMOST,
            Some(InsertAfter::NoTopMost) => HWND_NOTOPMOST,
            None => {
                flags |= SWP_NOZORDER;
                0
            }
        };

        // Determine the position field/flag.
        let [x, y] = match position {
            Some(posn) => posn.into(),
            None => {
                flags |= SWP_NOMOVE;
                [0, 0]
            }
        };

        // Determine the size field/flag.
        let [width, height] = match size {
            Some(size) => size.into(),
            None => {
                flags |= SWP_NOSIZE;
                [0, 0]
            }
        };

        // Set the window position.
        let result = unsafe {
            SetWindowPos(
                self.as_window().hwnd,
                insert_after,
                x,
                y,
                width,
                height,
                flags,
            )
        };

        if result == 0 {
            Err(Error::last_error("SetWindowPos"))
        } else {
            Ok(())
        }
    }

    /// Convert a point from screen coordinates to client coordinates.
    fn client_to_screen(&self, mut point: Point<i32>) -> Result<Point<i32>, Error> {
        let result =
            unsafe { ClientToScreen(self.as_window().hwnd, &mut point as *mut _ as *mut _) };

        if result == 0 {
            Err(Error::last_error("ClientToScreen"))
        } else {
            Ok(point)
        }
    }

    /// Convert a point from client coordinates to screen coordinates.
    fn screen_to_client(&self, mut point: Point<i32>) -> Result<Point<i32>, Error> {
        let result =
            unsafe { ScreenToClient(self.as_window().hwnd, &mut point as *mut _ as *mut _) };

        if result == 0 {
            Err(Error::last_error("ScreenToClient"))
        } else {
            Ok(point)
        }
    }

    /// Get a DC for this window.
    fn get_dc(
        &self,
        region: RegionType,
        flags: GetDcFlags,
    ) -> Result<DeviceContext<GetReleaser<'_>>, Error> {
        DeviceContext::get_dc(Some(self.as_window()), region, flags)
    }
}

impl AsWindow for BorrowedWindow<'_> {
    fn as_window(&self) -> BorrowedWindow<'_> {
        unsafe { Self::from_raw_handle(self.hwnd) }
    }
}

impl<T> AsWindow for Window<'_, T> {
    fn as_window(&self) -> BorrowedWindow<'_> {
        unsafe { BorrowedWindow::from_raw_handle(self.hwnd) }
    }
}

#[cfg(feature = "raw-window-handle")]
unsafe impl raw_window_handle::HasRawWindowHandle for BorrowedWindow<'_> {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        let mut handle = raw_window_handle::Win32WindowHandle::empty();
        handle.hwnd = strict::invalid(self.hwnd) as *mut _;
        handle.hinstance = strict::invalid(current_module()) as *mut _;

        raw_window_handle::RawWindowHandle::Win32(handle)
    }
}

#[cfg(feature = "raw-window-handle")]
unsafe impl<T> raw_window_handle::HasRawWindowHandle for Window<'_, T> {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.as_window().raw_window_handle()
    }
}

bitflags::bitflags! {
    /// Window styles.
    pub struct WindowStyle : u32 {
        /// The window has a thin-line border.
        const BORDER = WS_BORDER;

        /// The window has a title bar.
        const CAPTION = WS_CAPTION;

        /// The window is a child window.
        const CHILD = WS_CHILD;

        /// Excludes the area occupied by child windows.
        const CLIP_CHILDREN = WS_CLIPCHILDREN;

        /// Excludes the area occupied by sibling windows.
        const CLIP_SIBLINGS = WS_CLIPSIBLINGS;

        /// The window is initially disabled.
        const DISABLED = WS_DISABLED;

        /// The window has a border of a style typically used with dialog boxes.
        const DIALOG_FRAME = WS_DLGFRAME;

        /// The window is the first control of a group of controls.
        const GROUP = WS_GROUP;

        /// The window has a horizontal scroll bar.
        const H_SCROLL = WS_HSCROLL;

        /// The window is initially maximized.
        const MAXIMIZE = WS_MAXIMIZE;

        /// The window has a maximize button.
        const MAXIMIZE_BOX = WS_MAXIMIZEBOX;

        /// The window is initially minimized.
        const MINIMIZE = WS_MINIMIZE;

        /// The window has a minimize button.
        const MINIMIZE_BOX = WS_MINIMIZEBOX;

        /// The window is an overlapped window.
        const OVERLAPPED = WS_OVERLAPPED;

        /// The window is an overlapped window.
        const OVERLAPPED_WINDOW = WS_OVERLAPPEDWINDOW;

        /// The window is a pop-up window.
        const POPUP = WS_POPUP;

        /// The window is a pop-up window.
        const POPUP_WINDOW = WS_POPUPWINDOW;

        /// The window has a sizing border.
        const SIZE_BOX = WS_SIZEBOX;

        /// The window has a control that receives keyboard focus when the user presses the TAB key.
        const TAB_STOP = WS_TABSTOP;

        /// The window has a sizing border.
        const THICK_FRAME = WS_THICKFRAME;

        /// The window is initially visible.
        const VISIBLE = WS_VISIBLE;

        /// The window has a vertical scroll bar.
        const V_SCROLL = WS_VSCROLL;
    }
}

bitflags::bitflags! {
    /// Extended window styles.
    pub struct ExtendedStyle : u32 {
        /// The window accepts drag-drop files.
        const ACCEPT_FILES = WS_EX_ACCEPTFILES;

        /// Forces a top-level window onto the taskbar when the window is visible.
        const APP_WINDOW = WS_EX_APPWINDOW;

        /// The window has a border with a sunken edge.
        const CLIENT_EDGE = WS_EX_CLIENTEDGE;

        /// Paints all descendants of a window in bottom-to-top painting order using double-buffering.
        const COMPOSITED = WS_EX_COMPOSITED;

        /// The title bar of the window includes a question mark.
        const CONTEXT_HELP = WS_EX_CONTEXTHELP;

        /// The window itself contains child windows that should take part in dialog box navigation.
        const CONTROLS_PARENT = WS_EX_CONTROLPARENT;

        /// The window has a double border.
        const DLG_MODAL_FRAME = WS_EX_DLGMODALFRAME;

        /// The window is a layered window.
        const LAYERED = WS_EX_LAYERED;

        /// The window does not pass its window layout to its child windows.
        const LAYOUT_RTL = WS_EX_LAYOUTRTL;

        /// If the shell language is Hebrew, Arabic, or another language that supports reading order
        /// alignment, the horizontal origin of the window is on the right edge.
        const LEFT_TO_RIGHT_READING = WS_EX_LEFT;

        /// Scroll bar on the left?
        const LEFT_SCROLL_BAR = WS_EX_LEFTSCROLLBAR;

        /// The window is an MDI child window.
        const MDI_CHILD = WS_EX_MDICHILD;

        /// A top-level window created with this style does not become the foreground window when the
        /// user clicks it.
        const NO_ACTIVATE = WS_EX_NOACTIVATE;

        /// The window does not pass its window layout to its child windows.
        const NO_INHERIT_LAYOUT = WS_EX_NOINHERITLAYOUT;

        /// The child window created with this style does not send the WM_PARENTNOTIFY message to its
        /// parent window when it is created or destroyed.
        const NO_PARENT_NOTIFY = WS_EX_NOPARENTNOTIFY;

        /// The window does not render to a redirection surface.
        const NO_REDIRECTION_BITMAP = WS_EX_NOREDIRECTIONBITMAP;

        /// The window is an overlapped window.
        const OVERLAPPED_WINDOW = WS_EX_OVERLAPPEDWINDOW;

        /// The window is palette window, which is a modeless dialog box that presents an array of
        /// commands.
        const PALETTE_WINDOW = WS_EX_PALETTEWINDOW;

        /// The window has generic "right-aligned" properties. This depends on the window class.
        const RIGHT = WS_EX_RIGHT;

        /// If the shell language is Hebrew, Arabic, or another language that supports reading order
        /// alignment, the vertical scroll bar (if present) is to the left of the client area.
        const RIGHT_SCROLL_BAR = WS_EX_RIGHTSCROLLBAR;

        /// The window text is displayed using left-to-right reading-order properties.
        const RTL_LAYOUT = WS_EX_RTLREADING;

        /// The window has a three-dimensional border style intended to be used for items that do not
        /// accept user input.
        const STATIC_EDGE = WS_EX_STATICEDGE;

        /// The window is intended to be used as a floating toolbar.
        const TOOL_WINDOW = WS_EX_TOOLWINDOW;

        /// The window should be placed above all non-topmost windows and should stay above them.
        const TOPMOST = WS_EX_TOPMOST;

        /// The window should not be painted until siblings beneath the window (that were created by
        /// the same thread) have been painted.
        const TRANSPARENT = WS_EX_TRANSPARENT;

        /// The window has a border with a raised edge.
        const WINDOW_EDGE = WS_EX_WINDOWEDGE;
    }
}

bitflags::bitflags! {
    /// Commands to send to the window.
    pub struct ShowCommand : u32 {
        /// Hide the window.
        const HIDE = SW_HIDE;

        /// Show the window.
        const NORMAL = SW_NORMAL;

        /// Show and minimize the window.
        const SHOW_MINIMIZED = SW_SHOWMINIMIZED;

        /// Show and minimize the window.
        const MINIMIZED = SW_MINIMIZE;

        /// Show and maximize the window.
        const MAXIMIZED = SW_SHOWMAXIMIZED;

        /// Show the window but don't activate it.
        const NO_ACTIVATE = SW_SHOWNOACTIVATE;

        /// Show the window in its current state.
        const SHOW = SW_SHOW;

        /// Show the window using the default state.
        const SHOW_DEFAULT = SW_SHOWDEFAULT;

        /// Force the window to be minimized.
        const FORCE_MINIMIZED = SW_FORCEMINIMIZE;
    }
}

bitflags::bitflags! {
    /// Flags for the `get_dc` function.
    pub struct GetDcFlags: u32 {
        /// Get the window area instead of the client area.
        const WINDOW_AREA = DCX_WINDOW;

        /// Returns a cached DC no matter what.
        const CACHE = DCX_CACHE;

        /// Use the visible region of the parent window.
        const PARENT_CLIP = DCX_PARENTCLIP;

        /// Exclude the visible regions of all sibling windows above the window.
        const CLIP_SIBLINGS = DCX_CLIPSIBLINGS;

        /// Exclude the visible regions of all child windows below the window.
        const CLIP_CHILDREN = DCX_CLIPCHILDREN;

        /// Allows drawing even if there is a LockWindowUpdate call in effect that would otherwise
        /// exclude this window.
        const LOCK_WINDOW_UPDATE = DCX_LOCKWINDOWUPDATE;
    }
}

/// The type of region clipping to do for `GetDCEx`.
pub enum RegionType {
    /// No clipping.
    None,

    /// Intersect the region with the visible region.
    Intersect(Region),

    /// Exclude the region from the visible region.
    Exclude(Region),
}

/// The handle to insert the window after.
#[derive(Debug, Copy, Clone)]
pub enum InsertAfter<'hwnd> {
    /// The window to insert before.
    Window(BorrowedWindow<'hwnd>),

    /// Place the window at the bottom of the Z order.
    Bottom,

    /// Place the window at the top of the Z order.
    Top,

    /// Place the window above all non-topmost windows.
    TopMost,

    //// Place the window above all non-topmost windows without making it topmost.
    NoTopMost,
}

bitflags::bitflags! {
    /// Flags for `SetWindowPos`.
    pub struct WindowPosFlags : u32 {
        /// Prevents generation of the WM_SYNCPAINT message.
        const DEFER_ERASE = SWP_DEFERERASE;

        /// Draw a frame around the window.
        const DRAW_FRAME = SWP_DRAWFRAME;

        /// Applies new frame styles set using the SetWindowLong function.
        const FRAME_CHANGED = SWP_FRAMECHANGED;

        /// Hides the window.
        const HIDE_WINDOW = SWP_HIDEWINDOW;

        /// Does not activate the window.
        const NO_ACTIVATE = SWP_NOACTIVATE;

        /// Discards the entire contents of the client area.
        const NO_COPY_BITS = SWP_NOCOPYBITS;

        /// Do not change the owner window's position in the Z order.
        const NO_OWNER_Z_ORDER = SWP_NOOWNERZORDER;

        /// Do not redraw changes.
        const NO_REDRAW = SWP_NOREDRAW;

        /// Do not send the WM_WINDOWPOSCHANGING message to the window being repositioned.
        const NO_SEND_CHANGING = SWP_NOSENDCHANGING;

        /// Display the window.
        const SHOW_WINDOW = SWP_SHOWWINDOW;
    }
}

#[repr(C)]
pub(crate) struct WindowData<'a, T> {
    /// Propogate a panic from the window procedure to the main thread.
    ///
    /// This comes first in order to allow us to call it, like in a VTable.
    propagate_panic: fn(*const ()),

    /// The handle to the window.
    hwnd: HWND,

    /// A queue of messages to be processed.
    message_queue: RefCell<VecDeque<Event<'static>>>,

    /// The user data associated with the window.
    user_data: Box<T>,

    /// The class data for the window.
    class_data: Rc<dyn ErasedClassData<T> + 'a>,

    /// The re-entrancy count of the current window procedure.
    rentrancy_count: Cell<Option<NonZeroU32>>,

    /// The latest panic that occurred in the window's event loop, if any.
    #[cfg(feature = "std")]
    panic: Cell<Option<Box<dyn Any + Send>>>,
}

// With libstd, we can catch panics to prevent them from hitting the abort guard.
#[cfg(feature = "std")]
impl<'a, T> WindowData<'a, T> {
    /// Propagate a panic if one occurred.
    pub(crate) fn propagate_panic(&self) {
        if let Some(panic) = self.panic.take() {
            std::panic::resume_unwind(panic);
        }
    }

    /// Run code and store the panic if one happened.
    pub(crate) fn catch_panic<F: FnOnce()>(&self, f: F) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(panic) = result {
            if let Some(other_panic) = self.panic.replace(Some(panic)) {
                abort!("Two simultaneous panics in the same event procedure.");
            }
        }
    }
}

// Without libstd, we can't propagate panics. Just let it hit the abort guard.
#[cfg(not(feature = "std"))]
impl<'a, T> WindowData<'a, T> {
    /// Propagate a panic if one occurred.
    #[inline]
    pub(crate) fn propagate_panic(&self) {}

    /// Run code and store the panic if one happened.
    #[inline]
    pub(crate) fn catch_panic<F: FnOnce()>(&self, f: F) {
        f();
    }
}

impl<'a, T> WindowData<'a, T> {
    /// Create a new window data.
    pub(crate) fn new<F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>) + 'a>(
        hwnd: HWND,
        data: Box<T>,
        class_data: Rc<ClassData<F>>,
    ) -> Self {
        Self {
            propagate_panic: |ptr| {
                let data: &WindowData<'_, T> = unsafe { &*(ptr as *const _) };
                data.propagate_panic();
            },
            hwnd,
            message_queue: RefCell::new(VecDeque::new()),
            user_data: data,
            class_data,
            rentrancy_count: Cell::new(None),
            #[cfg(feature = "std")]
            panic: Cell::new(None),
        }
    }

    /// Push a new event.
    pub(crate) fn push(&self, event: Event<'static>) {
        self.message_queue.borrow_mut().push_back(event);
    }

    /// Process all events.
    fn process(&self) {
        let mut queue = self.message_queue.borrow_mut();
        while let Some(event) = queue.pop_front() {
            self.class_data.run_handler(
                &self.user_data,
                unsafe { BorrowedWindow::from_raw_handle(self.hwnd) },
                event,
            );
        }
    }

    /// Begin a new re-entrancy scope.
    pub(crate) fn begin(&self) -> impl Drop + '_ {
        struct CallOnDrop<F: Fn()>(F);

        impl<F: Fn()> Drop for CallOnDrop<F> {
            fn drop(&mut self) {
                (self.0)();
            }
        }

        let guard = CallOnDrop(move || {
            let current_count = match self.rentrancy_count.get() {
                Some(count) => count.get(),
                None => panic!("Re-entrancy count is not set."),
            };

            match current_count - 1 {
                0 => {
                    self.rentrancy_count.set(None);
                    self.process();
                }
                n => self
                    .rentrancy_count
                    .set(Some(unsafe { NonZeroU32::new_unchecked(n) })),
            }
        });

        // Bump the re-entrancy count.
        match self.rentrancy_count.get() {
            Some(count) => self.rentrancy_count.set(Some(unsafe {
                NonZeroU32::new_unchecked(
                    count
                        .get()
                        .checked_add(1)
                        .expect("Re-entrancy count overflowed."),
                )
            })),
            None => self
                .rentrancy_count
                .set(Some(unsafe { NonZeroU32::new_unchecked(1) })),
        }

        guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::ClassBuilder;
    use crate::cstr::CString;
    use crate::event::Event;
    use crate::Client;

    #[test]
    fn test_window() {
        let client = Client::new();
        let class_name = CString::new("test_window_creation").unwrap();
        let window_title = CString::new("test_window_creation").unwrap();
        let class = client
            .create_class(&class_name)
            .build(|client, &(), _, ev| {
                if let Event::Created = ev {
                    client.quit();
                }
            })
            .expect("Failed to create window class");

        // Create the window.
        let _window = client
            .create_window(
                &class,
                &window_title,
                None,
                None,
                WindowStyle::empty(),
                ExtendedStyle::empty(),
                Rect::new(Point::new(0, 0), Size::new(1, 1)),
                (),
            )
            .expect("Failed to create window");

        // Run the client.
        crate::reactor::Reactor::new()
            .expect("to create client")
            .run()
            .expect("to run without errors");
    }
}
