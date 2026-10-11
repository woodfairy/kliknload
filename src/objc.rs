//! Minimal Objective-C runtime bindings (no bindings crate), shared by the
//! notification and login item code.

use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::OnceLock;

pub type Id = *mut c_void;
pub type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    pub fn objc_getClass(name: *const c_char) -> Id;
    pub fn objc_getProtocol(name: *const c_char) -> Id;
    pub fn sel_registerName(name: *const c_char) -> Sel;
    pub fn objc_msgSend();
    pub fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    pub fn objc_registerClassPair(class: Id);
    pub fn class_addMethod(class: Id, name: Sel, imp: *const c_void, types: *const c_char) -> bool;
    pub fn class_addProtocol(class: Id, protocol: Id) -> bool;
    pub fn objc_autoreleasePoolPush() -> *mut c_void;
    pub fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

pub fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("no NUL bytes left")
}

pub fn sel(name: &str) -> Sel {
    let c = cstring(name);
    unsafe { sel_registerName(c.as_ptr()) }
}

pub fn class(name: &str) -> Id {
    let c = cstring(name);
    unsafe { objc_getClass(c.as_ptr()) }
}

/// `objc_msgSend` has to be called through a pointer of the exact method signature.
macro_rules! msg {
    ($obj:expr, $sel:expr $(, $arg:expr => $ty:ty)* ; -> $ret:ty) => {{
        let f: unsafe extern "C" fn($crate::objc::Id, $crate::objc::Sel $(, $ty)*) -> $ret =
            std::mem::transmute($crate::objc::objc_msgSend as unsafe extern "C" fn());
        f($obj, $crate::objc::sel($sel) $(, $arg)*)
    }};
}
pub(crate) use msg;

pub unsafe fn nsstring(s: &str) -> Id {
    let c = cstring(s);
    unsafe { msg!(class("NSString"), "stringWithUTF8String:", c.as_ptr() => *const c_char; -> Id) }
}

/// Copies an `NSString` into a Rust string (empty for nil).
pub unsafe fn string(s: Id) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let utf8 = msg!(s, "UTF8String"; -> *const c_char);
        if utf8.is_null() {
            String::new()
        } else {
            CStr::from_ptr(utf8).to_string_lossy().into_owned()
        }
    }
}

/// True when running from an `.app` bundle with a bundle identifier.
pub fn bundled() -> bool {
    static BUNDLED: OnceLock<bool> = OnceLock::new();
    *BUNDLED.get_or_init(|| unsafe {
        let pool = objc_autoreleasePoolPush();
        let bundle = msg!(class("NSBundle"), "mainBundle"; -> Id);
        let ok = !bundle.is_null()
            && !msg!(bundle, "bundleIdentifier"; -> Id).is_null()
            && string(msg!(bundle, "bundlePath"; -> Id)).ends_with(".app");
        objc_autoreleasePoolPop(pool);
        ok
    })
}
