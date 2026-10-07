//! Notification Center via the UserNotifications framework, called through the
//! Objective-C runtime directly (no bindings crate).
//!
//! UNUserNotificationCenter only works inside an app bundle. When kliknload runs as a
//! plain binary (e.g. `cargo run`, `--headless` in a terminal), `osascript` is used.

use anyhow::{Result, bail};
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::OnceLock;
use tracing::{info, warn};

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn objc_getProtocol(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    fn objc_registerClassPair(class: Id);
    fn class_addMethod(class: Id, name: Sel, imp: *const c_void, types: *const c_char) -> bool;
    fn class_addProtocol(class: Id, protocol: Id) -> bool;
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

// Provided by libSystem (libclosure), the isa of blocks without captures.
unsafe extern "C" {
    static _NSConcreteGlobalBlock: c_void;
}

// UNAuthorizationOptions
const AUTH_SOUND: usize = 1 << 1;
const AUTH_ALERT: usize = 1 << 2;
// UNNotificationPresentationOptions
const PRESENT_LIST: usize = 1 << 3;
const PRESENT_BANNER: usize = 1 << 4;

const BLOCK_IS_GLOBAL: i32 = 1 << 28;

/// Memory layout of an Objective-C block (Clang ABI).
#[repr(C)]
struct Block<F> {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: F,
    descriptor: *const BlockDescriptor,
}

#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
}

/// A capture-less block that lives forever; enough for completion handlers.
fn global_block<F>(invoke: F) -> *const Block<F> {
    let descriptor = Box::leak(Box::new(BlockDescriptor {
        reserved: 0,
        size: std::mem::size_of::<Block<F>>(),
    }));
    Box::leak(Box::new(Block {
        isa: &raw const _NSConcreteGlobalBlock,
        flags: BLOCK_IS_GLOBAL,
        reserved: 0,
        invoke,
        descriptor,
    }))
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("no NUL bytes left")
}

fn sel(name: &str) -> Sel {
    let c = cstring(name);
    unsafe { sel_registerName(c.as_ptr()) }
}

fn class(name: &str) -> Id {
    let c = cstring(name);
    unsafe { objc_getClass(c.as_ptr()) }
}

/// `objc_msgSend` has to be called through a pointer of the exact method signature.
macro_rules! msg {
    ($obj:expr, $sel:expr $(, $arg:expr => $ty:ty)* ; -> $ret:ty) => {{
        let f: unsafe extern "C" fn(Id, Sel $(, $ty)*) -> $ret =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        f($obj, sel($sel) $(, $arg)*)
    }};
}

unsafe fn nsstring(s: &str) -> Id {
    let c = cstring(s);
    unsafe { msg!(class("NSString"), "stringWithUTF8String:", c.as_ptr() => *const c_char; -> Id) }
}

/// True when running from an `.app` bundle with a bundle identifier.
fn bundled() -> bool {
    static BUNDLED: OnceLock<bool> = OnceLock::new();
    *BUNDLED.get_or_init(|| unsafe {
        let pool = objc_autoreleasePoolPush();
        let bundle = msg!(class("NSBundle"), "mainBundle"; -> Id);
        let ok = !bundle.is_null() && !msg!(bundle, "bundleIdentifier"; -> Id).is_null() && {
            let path = msg!(bundle, "bundlePath"; -> Id);
            let utf8 = msg!(path, "UTF8String"; -> *const c_char);
            !utf8.is_null() && CStr::from_ptr(utf8).to_string_lossy().ends_with(".app")
        };
        objc_autoreleasePoolPop(pool);
        ok
    })
}

/// `userNotificationCenter:willPresentNotification:withCompletionHandler:`
/// Shows banners even while kliknload is the active app (e.g. its menu is open).
unsafe extern "C" fn will_present(
    _this: Id,
    _cmd: Sel,
    _center: Id,
    _notification: Id,
    handler: *mut Block<unsafe extern "C" fn(*mut c_void, usize)>,
) {
    if !handler.is_null() {
        unsafe { ((*handler).invoke)(handler.cast(), PRESENT_BANNER | PRESENT_LIST) };
    }
}

unsafe extern "C" fn authorization_done(_block: *mut c_void, granted: bool, error: Id) {
    if granted {
        info!("notification permission granted");
    } else if error.is_null() {
        warn!("notifications are disabled for kliknload in System Settings → Notifications");
    } else {
        unsafe {
            let desc = msg!(error, "localizedDescription"; -> Id);
            let utf8 = msg!(desc, "UTF8String"; -> *const c_char);
            let text = if utf8.is_null() {
                String::new()
            } else {
                CStr::from_ptr(utf8).to_string_lossy().into_owned()
            };
            warn!("notification permission request failed: {text}");
        }
    }
}

/// Registers the delegate and asks for permission (once).
pub fn init() {
    static INIT: OnceLock<()> = OnceLock::new();
    if !bundled() {
        return;
    }
    INIT.get_or_init(|| unsafe {
        let pool = objc_autoreleasePoolPush();
        let center = msg!(class("UNUserNotificationCenter"), "currentNotificationCenter"; -> Id);

        let name = cstring("KliknloadNotificationDelegate");
        let delegate_class = objc_allocateClassPair(class("NSObject"), name.as_ptr(), 0);
        if !delegate_class.is_null() {
            let types = cstring("v@:@@@?");
            class_addMethod(
                delegate_class,
                sel("userNotificationCenter:willPresentNotification:withCompletionHandler:"),
                will_present as *const c_void,
                types.as_ptr(),
            );
            let protocol = objc_getProtocol(cstring("UNUserNotificationCenterDelegate").as_ptr());
            if !protocol.is_null() {
                class_addProtocol(delegate_class, protocol);
            }
            objc_registerClassPair(delegate_class);
            // Never released: the center only keeps a weak reference.
            let delegate = msg!(msg!(delegate_class, "alloc"; -> Id), "init"; -> Id);
            msg!(center, "setDelegate:", delegate => Id; -> ());
        }

        let done = global_block(authorization_done as unsafe extern "C" fn(*mut c_void, bool, Id));
        msg!(center, "requestAuthorizationWithOptions:completionHandler:",
            AUTH_ALERT | AUTH_SOUND => usize, done.cast::<c_void>() => *const c_void; -> ());
        objc_autoreleasePoolPop(pool);
    });
}

pub fn notify(title: &str, message: &str) -> Result<()> {
    if !bundled() {
        return osascript(title, message);
    }
    init();
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let center = msg!(class("UNUserNotificationCenter"), "currentNotificationCenter"; -> Id);
        let content =
            msg!(msg!(class("UNMutableNotificationContent"), "alloc"; -> Id), "init"; -> Id);
        if center.is_null() || content.is_null() {
            objc_autoreleasePoolPop(pool);
            bail!("UserNotifications not available");
        }
        msg!(content, "setTitle:", nsstring(title) => Id; -> ());
        msg!(content, "setBody:", nsstring(message) => Id; -> ());
        let identifier = msg!(msg!(class("NSUUID"), "UUID"; -> Id), "UUIDString"; -> Id);
        let request = msg!(class("UNNotificationRequest"), "requestWithIdentifier:content:trigger:",
            identifier => Id, content => Id, std::ptr::null_mut() => Id; -> Id);
        msg!(center, "addNotificationRequest:withCompletionHandler:",
            request => Id, std::ptr::null_mut() => Id; -> ());
        msg!(content, "release"; -> ());
        objc_autoreleasePoolPop(pool);
    }
    Ok(())
}

/// Plain binaries outside a bundle: Notification Center through AppleScript.
fn osascript(title: &str, message: &str) -> Result<()> {
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let script = format!(
        "display notification {} with title {}",
        quote(message),
        quote(title)
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .spawn()?;
    Ok(())
}
