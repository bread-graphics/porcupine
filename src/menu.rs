// Boost/Apache2 License

//! Build and manipulate menus.

use crate::bitmap::Bitmap;
use crate::cstr::CStr;
use crate::Error;
use core::mem;

use windows_sys::Win32::UI::WindowsAndMessaging::{CreateMenu, DestroyMenu, InsertMenuItemA};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HBMMENU_MBAR_CLOSE, HBMMENU_MBAR_CLOSE_D, HBMMENU_MBAR_MINIMIZE, HBMMENU_MBAR_MINIMIZE_D,
    HBMMENU_MBAR_RESTORE, HBMMENU_POPUP_CLOSE, HBMMENU_POPUP_MAXIMIZE, HBMMENU_POPUP_MINIMIZE,
    HBMMENU_POPUP_RESTORE, MFS_CHECKED, MFS_DEFAULT, MFS_DISABLED, MFS_HILITE, MFT_MENUBARBREAK,
    MFT_MENUBREAK, MFT_RADIOCHECK, MFT_RIGHTJUSTIFY, MFT_RIGHTORDER, MFT_SEPARATOR, MIIM_BITMAP,
    MIIM_CHECKMARKS, MIIM_FTYPE, MIIM_STATE, MIIM_STRING, MIIM_SUBMENU,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{HMENU, MENUITEMINFOA};

/*

TODO: In the future, we may want to consider owner-drawn menu items. However, it's a pain to do it
right, and even moreso to do it safely. As far as I can tell, there is no way to successfully drop
the backing memory for an owner-drawn menu item without leaking memory. There are ways around owner-
drawn items anyways, so I'm not going to worry about it for now.

If owner-drawn menus are important to you, I'd recommend re-evaluating your priorities. Cases where
owner-drawn menus are an absolute necessity are usually problems that can be solved in a better way
anyways. If you still need owner-drawn menus, open an issue.

*/

/// Actual system representation of a menu item.
struct MenuItemInfo {
    info: MENUITEMINFOA,
}

/// A menu.
pub struct Menu {
    handle: HMENU,
    len: usize,
}

/// A menu item.
pub struct MenuItem<'a> {
    /// The associated bitmap, if any.
    bitmap: Option<BitmapOptions>,

    /// The item type.
    item_type: MenuItemType<'a>,

    /// Whether or not this item is a checkmark.
    checkmark: Option<CheckmarksInfo>,

    /// Addition FTYPE flags.
    ftype: Option<Ftype>,

    /// Fstate flags.
    fstate: Option<Fstate>,

    /// Additional drop-down menu.
    submenu: Option<Menu>,
}

enum MenuItemType<'a> {
    /// Empty hole.
    Empty,

    /// This menu item is a string.
    String(&'a CStr),

    /// This menu item is a separator.
    Separator,

    /// This menu item is a menu break.
    MenuBreak,

    /// This menu item is a menu bar break.
    MenuBarBreak,
}

struct CheckmarksInfo {
    /// Checked bitmap.
    checked: Option<Bitmap>,

    /// Unchecked bitmap.
    unchecked: Option<Bitmap>,

    /// Whether this is a radio button.
    radio: bool,
}

bitflags::bitflags! {
    /// Public `fTtype` flags.
    pub struct Ftype : u32 {
        /// This menu item is right-aligned.
        const RIGHT_JUSTIFY = MFT_RIGHTJUSTIFY;

        /// This menu item cascades from left to right.
        const RIGHT_ORDER = MFT_RIGHTORDER;
    }
}

bitflags::bitflags! {
    /// Public `fState` flags.
    pub struct Fstate : u32 {
        /// This menu item is checked.
        const CHECKED = MFS_CHECKED;

        /// The menu item is the default.
        const DEFAULT = MFS_DEFAULT;

        /// This menu item is disabled.
        const DISABLED = MFS_DISABLED;

        /// This menu item is highlighted.
        const HIGHLIGHTED = MFS_HILITE;
    }
}

/// Bitmap options for a menu item.
pub enum BitmapOptions {
    /// A handle to a bitmap.
    Handle(Bitmap),

    /// Close button.
    Close {
        /// Whether or not the button is disabled.
        disabled: bool,
    },

    /// Minimize button,
    Minimize {
        /// Whether or not the button is disabled.
        disabled: bool,
    },

    /// Restore button.
    Restore,

    /// Close popup button.
    ClosePopup,

    /// Maximize popup button.
    MaximizePopup,

    /// Minimize popup button.
    MinimizePopup,

    /// Restore popup button.
    RestorePopup,
}

impl From<Bitmap> for BitmapOptions {
    fn from(bitmap: Bitmap) -> Self {
        Self::Handle(bitmap)
    }
}

impl<'a> MenuItem<'a> {
    /// Create a new `MenuItem` from a `MenuItemType`.
    fn new(item_type: MenuItemType<'a>) -> Self {
        Self {
            bitmap: None,
            item_type,
            checkmark: None,
            fstate: None,
            ftype: None,
            submenu: None,
        }
    }

    /// Set the bitmap for this menu item.
    pub fn bitmap(&mut self, bitmap: impl Into<BitmapOptions>) -> &mut Self {
        self.bitmap = Some(bitmap.into());
        self
    }

    /// Set the checkmark information for this menu item.
    pub fn checkbox(
        &mut self,
        checked: impl Into<Option<Bitmap>>,
        unchecked: impl Into<Option<Bitmap>>,
        radio: bool,
    ) -> &mut Self {
        self.checkmark = Some(CheckmarksInfo {
            checked: checked.into(),
            unchecked: unchecked.into(),
            radio,
        });
        self
    }

    /// Convert this menu item into a menu item info.
    fn take_info(&mut self) -> MenuItemInfo {
        let mut info: MENUITEMINFOA = unsafe { mem::zeroed() };
        info.cbSize = mem::size_of::<MENUITEMINFOA>() as _;

        // Set the menu item type.
        match mem::replace(&mut self.item_type, MenuItemType::Empty) {
            MenuItemType::String(item) => {
                info.fMask |= MIIM_STRING;
                info.dwTypeData = item.as_ptr() as _;
                info.cch = item.to_bytes().len() as _;
            }
            MenuItemType::Separator => {
                info.fMask |= MIIM_FTYPE;
                info.fType |= MFT_SEPARATOR;
            }
            MenuItemType::MenuBreak => {
                info.fMask |= MIIM_FTYPE;
                info.fType |= MFT_MENUBREAK;
            }
            MenuItemType::MenuBarBreak => {
                info.fMask |= MIIM_FTYPE;
                info.fType |= MFT_MENUBARBREAK;
            }
            MenuItemType::Empty => panic!("cannot poll an empty menu item"),
        }

        // Set the bitmap.
        if let Some(bitmap) = self.bitmap.take() {
            info.fMask |= MIIM_BITMAP;
            info.hbmpItem = match bitmap {
                BitmapOptions::Handle(bitmap) => bitmap.into_handle(),
                BitmapOptions::Close { disabled: false } => HBMMENU_MBAR_CLOSE,
                BitmapOptions::Close { disabled: true } => HBMMENU_MBAR_CLOSE_D,
                BitmapOptions::Minimize { disabled: false } => HBMMENU_MBAR_MINIMIZE,
                BitmapOptions::Minimize { disabled: true } => HBMMENU_MBAR_MINIMIZE_D,
                BitmapOptions::Restore => HBMMENU_MBAR_RESTORE,
                BitmapOptions::ClosePopup => HBMMENU_POPUP_CLOSE,
                BitmapOptions::MaximizePopup => HBMMENU_POPUP_MAXIMIZE,
                BitmapOptions::MinimizePopup => HBMMENU_POPUP_MINIMIZE,
                BitmapOptions::RestorePopup => HBMMENU_POPUP_RESTORE,
            };
        }

        // Set additional FTYPE information.
        if let Some(ftype) = self.ftype {
            info.fMask |= MIIM_FTYPE;
            info.fType |= ftype.bits();
        }

        // Set additional FSTATE information.
        if let Some(fstate) = self.fstate {
            info.fMask |= MIIM_STATE;
            info.fState |= fstate.bits();
        }

        // Set checkmark information.
        if let Some(checkmark) = self.checkmark.take() {
            info.fMask |= MIIM_CHECKMARKS;

            if let Some(checked) = checkmark.checked {
                info.hbmpChecked = checked.into_handle();
            }

            if let Some(unchecked) = checkmark.unchecked {
                info.hbmpUnchecked = unchecked.into_handle();
            }

            if checkmark.radio {
                info.fMask |= MIIM_FTYPE;
                info.fType |= MFT_RADIOCHECK;
            }
        }

        // Set the submenu.
        if let Some(submenu) = self.submenu.take() {
            info.fMask |= MIIM_SUBMENU;
            info.hSubMenu = submenu.handle;
            mem::forget(submenu);
        }

        MenuItemInfo { info }
    }

    /// Create a new menu item that uses a string.
    pub fn string(item: &'a CStr) -> Self {
        Self::new(MenuItemType::String(item))
    }

    /// Create a new menu item that is a separator.
    pub fn separator() -> Self {
        Self::new(MenuItemType::Separator)
    }

    /// Create a new menu item that is a menu break.
    pub fn menu_break() -> Self {
        Self::new(MenuItemType::MenuBreak)
    }

    /// Create a new menu item that is a menu bar break.
    pub fn menu_bar_break() -> Self {
        Self::new(MenuItemType::MenuBarBreak)
    }
}

impl Menu {
    /// Create a new, empty menu.
    pub fn new() -> Result<Self, Error> {
        let menu = unsafe { CreateMenu() };

        if menu == 0 {
            Err(Error::last_error("CreateMenu"))
        } else {
            Ok(Self {
                handle: menu,
                len: 0,
            })
        }
    }

    /// Insert a new item into the menu.
    pub fn insert(
        &mut self,
        index: u32,
        item: &mut MenuItem<'_>,
    ) -> Result<(), Error> {
        let info = item.take_info();
        let result = unsafe { InsertMenuItemA(self.handle, index, 1, &info.info) };

        if result == 0 {
            Err(Error::last_error("InsertMenuItemA"))
        } else {
            self.len = self.len.checked_add(1).unwrap_or_else(|| {
                panic!("menu item count overflowed");
            });

            if self.len >= u32::MAX as _ {
                panic!("menu item count overflowed");
            }

            Ok(())
        }
    }

    /// Push a new item onto the menu.
    pub fn push(&mut self, item: &mut MenuItem<'_>) -> Result<(), Error> {
        self.insert(self.len as _, item)
    }

    /// Number of items in the menu.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Is this menu empty?
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn into_handle(self) -> HMENU {
        let handle = self.handle;
        mem::forget(self);
        handle
    }
}

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe { DestroyMenu(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu() {
        let mut menu = Menu::new().unwrap();
        let mut item = MenuItem::string(CStr::from_bytes_with_nul(b"Hello\0").unwrap());
        menu.push(&mut item).unwrap();
        assert_eq!(menu.len(), 1);
    }
}
