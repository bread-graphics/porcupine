// Boost/Apache2 License

use crate::client::Client;
use crate::cstr::CStr;
use crate::event::Event;
use crate::module::current_module;
use crate::strict;
use crate::window::BorrowedWindow;
use crate::Error;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::marker::PhantomData;
use core::mem;
use core::ptr::{self, NonNull};

use windows_sys::Win32::UI::WindowsAndMessaging::WNDCLASSEXA;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DestroyWindow, RegisterClassExA, SetClassLongPtrA,
    UnregisterClassA,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_BYTEALIGNCLIENT, CS_BYTEALIGNWINDOW, CS_CLASSDC, CS_DBLCLKS, CS_DROPSHADOW, CS_GLOBALCLASS,
    CS_HREDRAW, CS_NOCLOSE, CS_OWNDC, CS_PARENTDC, CS_SAVEBITS, CS_VREDRAW, GCLP_WNDPROC,
};

/// A builder for a window class.
pub struct ClassBuilder<'a> {
    /// The inner class.
    inner: WNDCLASSEXA,

    /// The client.
    client: Client,

    /// Capture lifetime for string fields.
    _marker: PhantomData<&'a CStr>,
}

impl Client {
    /// Create a new window class.
    pub fn create_class<'a>(&self, class_name: &'a CStr) -> ClassBuilder<'a> {
        ClassBuilder::new(self, class_name)
    }
}

impl<'a> ClassBuilder<'a> {
    /// Create a new `ClassBuilder`.
    fn new(client: &Client, class_name: &'a CStr) -> Self {
        Self {
            inner: WNDCLASSEXA {
                cbSize: mem::size_of::<WNDCLASSEXA>() as u32,
                lpszClassName: class_name.as_ptr().cast(),
                hInstance: current_module(),
                ..unsafe { mem::zeroed() }
            },
            client: client.clone(),
            _marker: PhantomData,
        }
    }

    /// Set the name of this class.
    pub fn name(mut self, name: &'a CStr) -> Self {
        self.inner.lpszClassName = name.as_ptr().cast();
        self
    }

    /// Set the menu belonging to this class.
    pub fn menu(&mut self, menu: &'a CStr) -> &mut Self {
        self.inner.lpszMenuName = menu.as_ptr().cast();
        self
    }

    /// Set the style of this class.
    pub fn style(&mut self, style: Style) -> &mut Self {
        self.inner.style = style.bits();
        self
    }

    /// Construct the class with the given event handler and window-specific data.
    pub fn build<'evl, T: 'evl, F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>) + 'evl>(
        &self,
        handler: F,
    ) -> Result<WindowClass<'evl, T>, Error> {
        let mut cls = self.inner;

        // Set the class-specific size and the window handler.
        cls.cbClsExtra = mem::size_of::<Rc<ClassData<F>>>() as i32;
        cls.lpfnWndProc = Some(DefWindowProcA);

        // Register the class.
        let atom = unsafe { RegisterClassExA(&cls) };

        // Create a dummy window to manipulate the class data.
        let dummy_hwnd = unsafe {
            CreateWindowExA(
                0,
                strict::invalid(atom as _).cast(),
                ptr::null(),
                0,
                0,
                0,
                1,
                1,
                0,
                0,
                current_module(),
                ptr::null(),
            )
        };

        // Store the client and the event handler.
        let data = Rc::into_raw(Rc::new(ClassData {
            // We need to store information necessary to drop this struct anonymously.
            drop_handler: |ptr| unsafe {
                let data = Rc::<ClassData<F>>::from_raw(ptr.as_ptr().cast());
                drop(data);
            },
            client: self.client.clone(),
            handler,
        }));

        // Set the class data and the window procedure.
        unsafe {
            SetClassLongPtrA(dummy_hwnd, 0, strict::expose(data as *const _ as *const _));

            #[allow(clippy::fn_to_numeric_cast)]
            SetClassLongPtrA(
                dummy_hwnd,
                GCLP_WNDPROC,
                crate::wndproc::porcupine_window_procedure::<T, F> as isize,
            );
        }

        // Destroy the dummy window.
        unsafe {
            DestroyWindow(dummy_hwnd);
        }

        // Check for errors.
        if atom == 0 {
            Err(Error::last_error("RegisterClassEx"))
        } else {
            Ok(WindowClass {
                ptr: crate::strict::invalid(atom as isize).cast(),
                // We need to deallocate the event handler when the time comes.
                //
                // Since ClassData always has the drop function as its first field,
                // we can safely cast it to a function pointer pointer.
                drop_handler: unsafe { Some(NonNull::new_unchecked(data as *const _ as *mut _)) },
                _marker: PhantomData,
            })
        }
    }
}

bitflags::bitflags! {
    /// Bitflags for the `ClassBuilder::style` method.
    pub struct Style : u32 {
        /// Aligns the window's client area on a byte boundary (in the x direction).
        const BYTE_ALIGN_CLIENT = CS_BYTEALIGNCLIENT;

        /// Aligns the window on a byte boundary (in the x direction).
        const BYTE_ALIGN_WINDOW = CS_BYTEALIGNWINDOW;

        /// Allocates one device context to be shared by all windows in the class.
        const CLASS_DC = CS_CLASSDC;

        /// Sends a double-click message to the window procedure when the user double-clicks the mouse.
        const DOUBLE_CLICKS = CS_DBLCLKS;

        /// Enables the drop shadow effect on a window.
        const DROP_SHADOW = CS_DROPSHADOW;

        /// Indicates that the window class is an application global class.
        const GLOBAL_CLASS = CS_GLOBALCLASS;

        /// Redraws the entire window if a movement or size adjustment changes the width of the client area.
        const HREDRAW = CS_HREDRAW;

        /// Disables Close on the window menu.
        const NO_CLOSE = CS_NOCLOSE;

        /// Allocates a unique device context for each window in the class.
        const OWN_DC = CS_OWNDC;

        /// Sets the clipping rectangle of the child window to that of the parent window so that the child can draw on the parent.
        const PARENT_DC = CS_PARENTDC;

        /// Saves, as a bitmap, the portion of the screen image obscured by a window of this class.
        const SAVE_BITS = CS_SAVEBITS;

        /// Redraws the entire window if a movement or size adjustment changes the height of the client area.
        const VREDRAW = CS_VREDRAW;
    }
}

/// The class for a window.
///
/// The `T` parameter is the type of the window-specific data.
pub struct WindowClass<'a, T> {
    /// Either the class's atom or the class's name.
    ptr: *const u8,

    /// The pointer to the class data, if it exists and needs to be dropped.
    drop_handler: Option<NonNull<DropHandler>>,

    /// Lifetime for the class's name or the event handler.
    _marker: PhantomData<(&'a CStr, *const T)>,
}

impl<'a> WindowClass<'a, ()> {
    /// Create a new window class with the given name.
    #[inline]
    pub fn from_name(name: &'a CStr) -> Self {
        Self {
            ptr: name.as_ptr().cast(),
            drop_handler: None,
            _marker: PhantomData,
        }
    }
}

impl<'a, T> WindowClass<'a, T> {
    pub(crate) fn ptr(&self) -> *const u8 {
        self.ptr
    }
}

impl<'a, T> Drop for WindowClass<'a, T> {
    fn drop(&mut self) {
        // If we're storing event information, we need to drop it.
        if let Some(drop_handler) = self.drop_handler {
            // Try to deregister the class.
            let result = unsafe { UnregisterClassA(self.ptr, current_module()) };

            // This should only ever fail if someone's leaked a window. If so, abort.
            if result == 0 {
                abort!(
                    "UnregisterClass failed with error code {}",
                    Error::last_error("UnregisterClass")
                );
            }

            // We can now safely drop the event handler.
            let event_handler = drop_handler.cast();
            unsafe {
                (drop_handler.as_ref())(event_handler);
            }
        }
    }
}

type DropHandler = fn(NonNull<()>);

/// The data stored in the extra memory of a window class.
#[repr(C)]
pub(crate) struct ClassData<F> {
    /// The drop handler for the event handler.
    ///
    /// Putting this first allows us to drop the class data regardless of the
    /// event handler's type. This is basically what Rust's vtables do.
    drop_handler: DropHandler,

    /// The client.
    pub(crate) client: Client,

    /// The event handler.
    pub(crate) handler: F,
}

/// Generic trait over `ClassData`.
pub(crate) trait ErasedClassData<T> {
    /// Get the client from the class data.
    fn client(&self) -> &Client;

    /// Run the event handler.
    fn run_handler(&self, user_data: &T, window: BorrowedWindow<'_>, event: Event<'_>);
}

impl<T, F: Fn(&Client, &T, BorrowedWindow<'_>, Event<'_>)> ErasedClassData<T> for ClassData<F> {
    fn client(&self) -> &Client {
        &self.client
    }

    fn run_handler(&self, user_data: &T, window: BorrowedWindow<'_>, event: Event<'_>) {
        (self.handler)(&self.client, user_data, window, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cstr::CString;

    #[test]
    fn test_class_builder() {
        // Build a new class.
        let client = Client::new();
        let name = CString::new("test_class_builder").unwrap();
        let _class = ClassBuilder::new(&client, &name)
            .build(move |_, &(), _, _| {})
            .expect("Failed to build class");
    }
}
