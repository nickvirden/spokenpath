//! Every call into macOS the helper makes: the Accessibility API (`HIServices`), Core Foundation,
//! Core Graphics for the session's lock state, and the Objective-C runtime for Safari's process,
//! behind safe wrappers. This is the one module of the workspace that allows `unsafe` code.
//!
//! Every Core Foundation object the helper owns is held by [`Owned`], which releases it exactly
//! once. Nothing here creates the system-wide accessibility element, sets an attribute, or asks
//! macOS to prompt for the permission.

#![allow(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::ffi::{c_char, c_void};
use std::ptr::NonNull;

use crate::tree::{NumberValue, Raw};

type CFTypeRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type Boolean = u8;
type AXError = i32;

#[repr(C)]
#[derive(Default)]
struct CFRange {
    location: CFIndex,
    length: CFIndex,
}

#[repr(C)]
#[derive(Default)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Default)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Default)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;
/// `kCFNumberSInt64Type`.
const NUMBER_SINT64: CFIndex = 4;
/// `kCFNumberFloat64Type`.
const NUMBER_FLOAT64: CFIndex = 6;
/// `kAXValueCGPointType`, `kAXValueCGSizeType`, `kAXValueCGRectType` and `kAXValueCFRangeType`.
const AX_VALUE_POINT: u32 = 1;
const AX_VALUE_SIZE: u32 = 2;
const AX_VALUE_RECT: u32 = 3;
const AX_VALUE_RANGE: u32 = 4;
/// `kAXErrorSuccess`.
const AX_SUCCESS: AXError = 0;
/// How deep arrays inside arrays are encoded; a deeper one is recorded by its type alone.
const MAX_ARRAY_DEPTH: usize = 16;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFEqual(left: CFTypeRef, right: CFTypeRef) -> Boolean;
    fn CFHash(cf: CFTypeRef) -> usize;
    fn CFCopyTypeIDDescription(type_id: CFTypeID) -> CFTypeRef;
    safe fn CFStringGetTypeID() -> CFTypeID;
    fn CFStringGetLength(string: CFTypeRef) -> CFIndex;
    fn CFStringGetCharacters(string: CFTypeRef, range: CFRange, buffer: *mut u16);
    fn CFStringCreateWithBytes(
        allocator: CFTypeRef,
        bytes: *const u8,
        length: CFIndex,
        encoding: u32,
        is_external_representation: Boolean,
    ) -> CFTypeRef;
    safe fn CFBooleanGetTypeID() -> CFTypeID;
    fn CFBooleanGetValue(boolean: CFTypeRef) -> Boolean;
    safe fn CFNumberGetTypeID() -> CFTypeID;
    fn CFNumberIsFloatType(number: CFTypeRef) -> Boolean;
    fn CFNumberGetValue(number: CFTypeRef, number_type: CFIndex, value: *mut c_void) -> Boolean;
    safe fn CFURLGetTypeID() -> CFTypeID;
    fn CFURLCopyAbsoluteURL(url: CFTypeRef) -> CFTypeRef;
    fn CFURLGetString(url: CFTypeRef) -> CFTypeRef;
    safe fn CFArrayGetTypeID() -> CFTypeID;
    fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: CFIndex) -> CFTypeRef;
    safe fn CFDictionaryGetTypeID() -> CFTypeID;
    fn CFDictionaryGetValue(dictionary: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXIsProcessTrusted() -> Boolean;
    safe fn AXUIElementGetTypeID() -> CFTypeID;
    safe fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementCopyAttributeNames(element: CFTypeRef, names: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
    safe fn AXValueGetTypeID() -> CFTypeID;
    fn AXValueGetType(value: CFTypeRef) -> u32;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, out: *mut c_void) -> Boolean;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    safe fn CGSessionCopyCurrentDictionary() -> CFTypeRef;
}

// AppKit defines `NSRunningApplication`; linking it makes the class known to the runtime.
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *const c_void;
    safe fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
    // Declared without a signature: it is called only through a pointer cast to the signature of
    // the method it sends, as the Objective-C runtime requires.
    fn objc_msgSend();
}

/// A Core Foundation object the helper owns one reference to, released on drop.
struct Owned(NonNull<c_void>);

impl Owned {
    /// Takes ownership of an object returned by a `Create` or `Copy` function, or `None` for null.
    fn from_create(object: CFTypeRef) -> Option<Owned> {
        NonNull::new(object.cast_mut()).map(Owned)
    }

    /// Retains an object borrowed under the get rule, so it outlives its container.
    fn from_get(object: CFTypeRef) -> Option<Owned> {
        let object = NonNull::new(object.cast_mut())?;
        // SAFETY: `object` is a non-null Core Foundation object borrowed from a container the
        // caller still holds, so it is alive while it is retained.
        unsafe { CFRetain(object.as_ptr()) };
        Some(Owned(object))
    }

    fn as_ptr(&self) -> CFTypeRef {
        self.0.as_ptr()
    }

    fn type_id(&self) -> CFTypeID {
        // SAFETY: `self` owns a live Core Foundation object.
        unsafe { CFGetTypeID(self.as_ptr()) }
    }
}

impl Clone for Owned {
    fn clone(&self) -> Self {
        // SAFETY: `self` owns a live Core Foundation object; the retain gives the clone its own
        // reference, which its drop releases.
        unsafe { CFRetain(self.as_ptr()) };
        Owned(self.0)
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: `self` owns exactly one reference, taken at creation or by a retain, and drops
        // it exactly once here.
        unsafe { CFRelease(self.as_ptr()) };
    }
}

/// An immutable `CFString` the helper created.
pub(crate) struct CfString(Owned);

impl CfString {
    /// The `CFString` of `text`, or `None` if Core Foundation cannot allocate it.
    pub(crate) fn new(text: &str) -> Option<CfString> {
        let length = CFIndex::try_from(text.len()).ok()?;
        // SAFETY: the pointer and length describe the bytes of `text`, which is valid UTF-8 and
        // outlives the call; Core Foundation copies them. A null allocator is the default one.
        let string =
            unsafe { CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), length, UTF8, 0) };
        Owned::from_create(string).map(CfString)
    }
}

/// The UTF-16 code units of a `CFString`.
///
/// # Safety
///
/// `string` must be a live `CFString`.
unsafe fn utf16_of(string: CFTypeRef) -> Vec<u16> {
    // SAFETY: the caller guarantees `string` is a live `CFString`.
    let length = unsafe { CFStringGetLength(string) };
    let Ok(count) = usize::try_from(length) else {
        return Vec::new();
    };
    let mut units = vec![0_u16; count];
    // SAFETY: `units` holds exactly `length` code units, which is the range copied.
    unsafe {
        CFStringGetCharacters(
            string,
            CFRange {
                location: 0,
                length,
            },
            units.as_mut_ptr(),
        );
    }
    units
}

/// Whether this process holds the Accessibility permission. It never prompts.
pub(crate) fn process_trusted() -> bool {
    AXIsProcessTrusted() != 0
}

/// The session's `CGSSessionScreenIsLocked` value, `None` when the key or the session dictionary
/// is absent, as it is while the screen is unlocked.
pub(crate) fn screen_locked() -> Option<bool> {
    let session = Owned::from_create(CGSessionCopyCurrentDictionary())?;
    if session.type_id() != CFDictionaryGetTypeID() {
        return None;
    }
    let key = CfString::new("CGSSessionScreenIsLocked")?;
    // SAFETY: `session` is a live `CFDictionary` and `key` a live `CFString`; the value is
    // borrowed under the get rule and retained before `session` is released.
    let value = Owned::from_get(unsafe { CFDictionaryGetValue(session.as_ptr(), key.0.as_ptr()) })?;
    if value.type_id() == CFBooleanGetTypeID() {
        // SAFETY: `value` is a live `CFBoolean`.
        Some(unsafe { CFBooleanGetValue(value.as_ptr()) } != 0)
    } else if value.type_id() == CFNumberGetTypeID() {
        let mut number: i64 = 0;
        // SAFETY: `value` is a live `CFNumber` and `number` is an `i64`, which
        // `kCFNumberSInt64Type` writes.
        let read = unsafe {
            CFNumberGetValue(
                value.as_ptr(),
                NUMBER_SINT64,
                (&raw mut number).cast::<c_void>(),
            )
        };
        (read != 0).then_some(number != 0)
    } else {
        None
    }
}

/// An autorelease pool for the Objective-C calls, drained on drop.
struct Pool(*mut c_void);

impl Drop for Pool {
    fn drop(&mut self) {
        // SAFETY: the token came from `objc_autoreleasePoolPush` on this thread, and pools are
        // popped in reverse order because each lives in one function's scope.
        unsafe { objc_autoreleasePoolPop(self.0) };
    }
}

/// The pids of the running processes with Safari's bundle identifier in this user's session,
/// from `NSRunningApplication`, or why they cannot be listed.
pub(crate) fn safari_pids() -> Result<Vec<i32>, &'static str> {
    let _pool = Pool(objc_autoreleasePoolPush());
    // SAFETY: the name is a NUL-terminated C string literal.
    let class = unsafe { objc_getClass(c"NSRunningApplication".as_ptr()) };
    if class.is_null() {
        return Err("AppKit's NSRunningApplication class is not available");
    }
    // SAFETY: the names are NUL-terminated C string literals; the runtime interns them.
    let (by_bundle, process_identifier) = unsafe {
        (
            sel_registerName(c"runningApplicationsWithBundleIdentifier:".as_ptr()),
            sel_registerName(c"processIdentifier".as_ptr()),
        )
    };
    let bundle = CfString::new("com.apple.Safari").ok_or("the bundle identifier cannot be made")?;
    // SAFETY: `objc_msgSend` is called through the signature of the method it sends:
    // `+[NSRunningApplication runningApplicationsWithBundleIdentifier:]` takes an `NSString`
    // (toll-free bridged with `CFString`) and returns an autoreleased `NSArray`.
    let applications = unsafe {
        let send: unsafe extern "C" fn(*mut c_void, *const c_void, CFTypeRef) -> CFTypeRef =
            std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
        send(class, by_bundle, bundle.0.as_ptr())
    };
    if applications.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: an `NSArray` is toll-free bridged with `CFArray`, and the pool keeps it alive.
    let count = unsafe { CFArrayGetCount(applications) };
    let mut pids = Vec::new();
    for index in 0..count {
        // SAFETY: `index` is below the array's count; the item is borrowed while the pool lives.
        let application = unsafe { CFArrayGetValueAtIndex(applications, index) };
        if application.is_null() {
            continue;
        }
        // SAFETY: `-[NSRunningApplication processIdentifier]` takes no argument and returns a
        // `pid_t`, and `application` is an `NSRunningApplication` from the array.
        let pid = unsafe {
            let send: unsafe extern "C" fn(CFTypeRef, *const c_void) -> i32 =
                std::mem::transmute(objc_msgSend as unsafe extern "C" fn());
            send(application, process_identifier)
        };
        if pid > 0 {
            pids.push(pid);
        }
    }
    Ok(pids)
}

/// An accessibility element (`AXUIElement`) the helper holds a reference to.
#[derive(Clone)]
pub(crate) struct Element(Owned);

impl Element {
    /// The application element of the process `pid`.
    pub(crate) fn application(pid: i32) -> Option<Element> {
        Owned::from_create(AXUIElementCreateApplication(pid)).map(Element)
    }

    /// Bounds how long any one accessibility message to this element waits for an answer.
    pub(crate) fn set_messaging_timeout(&self, seconds: f32) -> AXError {
        // SAFETY: `self` holds a live `AXUIElement`.
        unsafe { AXUIElementSetMessagingTimeout(self.0.as_ptr(), seconds) }
    }

    /// The attribute's value, or the `AXError` the read failed with. A read that succeeds with no
    /// value is `kAXErrorNoValue`, as the API reports it.
    pub(crate) fn attribute(&self, name: &CfString) -> Result<Value, AXError> {
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: `self` holds a live `AXUIElement`, `name` is a live `CFString`, and `value` is
        // a valid out-pointer; on success the caller owns what it points to (copy rule).
        let error = unsafe {
            AXUIElementCopyAttributeValue(self.0.as_ptr(), name.0.as_ptr(), &raw mut value)
        };
        let owned = Owned::from_create(value);
        if error != AX_SUCCESS {
            return Err(error);
        }
        owned.map(Value).ok_or(-25212)
    }

    /// The names `AXUIElementCopyAttributeNames` returns, each with its `CFString`, or the
    /// `AXError` it failed with.
    pub(crate) fn attribute_names(&self) -> Result<Vec<(String, CfString)>, AXError> {
        let mut names: CFTypeRef = std::ptr::null();
        // SAFETY: `self` holds a live `AXUIElement` and `names` is a valid out-pointer; on
        // success the caller owns the array (copy rule).
        let error = unsafe { AXUIElementCopyAttributeNames(self.0.as_ptr(), &raw mut names) };
        let owned = Owned::from_create(names);
        if error != AX_SUCCESS {
            return Err(error);
        }
        let Some(array) = owned.filter(|array| array.type_id() == CFArrayGetTypeID()) else {
            return Ok(Vec::new());
        };
        // SAFETY: `array` is a live `CFArray`.
        let count = unsafe { CFArrayGetCount(array.as_ptr()) };
        let mut result = Vec::new();
        for index in 0..count {
            // SAFETY: `index` is below the count; the item is retained before `array` goes.
            let Some(name) =
                Owned::from_get(unsafe { CFArrayGetValueAtIndex(array.as_ptr(), index) })
            else {
                continue;
            };
            if name.type_id() == CFStringGetTypeID() {
                // SAFETY: `name` is a live `CFString`.
                let text = String::from_utf16_lossy(&unsafe { utf16_of(name.as_ptr()) });
                result.push((text, CfString(name)));
            }
        }
        Ok(result)
    }

    /// Whether the two are the same element.
    pub(crate) fn same(&self, other: &Element) -> bool {
        // SAFETY: both hold live `AXUIElement`s.
        unsafe { CFEqual(self.0.as_ptr(), other.0.as_ptr()) != 0 }
    }

    /// The element's hash, equal for elements that are the same.
    pub(crate) fn hash(&self) -> u64 {
        // SAFETY: `self` holds a live `AXUIElement`.
        let hash = unsafe { CFHash(self.0.as_ptr()) };
        u64::try_from(hash).unwrap_or(u64::MAX)
    }
}

/// An attribute value the helper owns.
pub(crate) struct Value(Owned);

impl Value {
    /// Whether the value is a `CFString` equal to `text`, compared by Core Foundation without
    /// copying the value out.
    pub(crate) fn is_string(&self, text: &CfString) -> bool {
        self.0.type_id() == CFStringGetTypeID()
            // SAFETY: both are live `CFString`s.
            && unsafe { CFEqual(self.0.as_ptr(), text.0.as_ptr()) != 0 }
    }

    /// The value, encoded by its type, with element references kept as elements.
    pub(crate) fn raw(&self) -> Raw<Element> {
        raw_of(&self.0, 0)
    }
}

/// The absolute string of a `CFURL`, owned.
fn absolute_string(url: &Owned) -> Option<Owned> {
    // SAFETY: `url` is a live `CFURL`; the copy rule gives the caller the absolute URL.
    let absolute = Owned::from_create(unsafe { CFURLCopyAbsoluteURL(url.as_ptr()) })?;
    // SAFETY: `absolute` is a live `CFURL`; its string is borrowed under the get rule and
    // retained before `absolute` is released.
    Owned::from_get(unsafe { CFURLGetString(absolute.as_ptr()) })
}

/// The description of the value's Core Foundation type.
fn type_description(value: &Owned) -> Vec<u16> {
    // SAFETY: the copy rule gives the caller the description string.
    let description = Owned::from_create(unsafe { CFCopyTypeIDDescription(value.type_id()) });
    description.map_or_else(Vec::new, |description| {
        // SAFETY: `description` is a live `CFString`.
        unsafe { utf16_of(description.as_ptr()) }
    })
}

/// Encodes one value by its type. Any type not named here, and an `AXValue` of another kind, is
/// recorded by its type's description, so no value is ever dropped or misread.
fn raw_of(value: &Owned, depth: usize) -> Raw<Element> {
    let type_id = value.type_id();
    let pointer = value.as_ptr();
    if type_id == CFStringGetTypeID() {
        // SAFETY: `value` is a live `CFString`.
        return Raw::String(unsafe { utf16_of(pointer) });
    }
    if type_id == CFBooleanGetTypeID() {
        // SAFETY: `value` is a live `CFBoolean`.
        return Raw::Boolean(unsafe { CFBooleanGetValue(pointer) } != 0);
    }
    if type_id == CFNumberGetTypeID() {
        return number(value);
    }
    if type_id == CFURLGetTypeID() {
        return absolute_string(value).map_or_else(
            || Raw::Unsupported(type_description(value)),
            // SAFETY: `string` is a live `CFString`.
            |string| Raw::Url(unsafe { utf16_of(string.as_ptr()) }),
        );
    }
    if type_id == AXUIElementGetTypeID() {
        return Raw::Element(Element(value.clone()));
    }
    if type_id == CFArrayGetTypeID() && depth < MAX_ARRAY_DEPTH {
        // SAFETY: `value` is a live `CFArray`.
        let count = unsafe { CFArrayGetCount(pointer) };
        let items = (0..count)
            .filter_map(|index| {
                // SAFETY: `index` is below the count; the item is retained before the array goes.
                Owned::from_get(unsafe { CFArrayGetValueAtIndex(pointer, index) })
            })
            .map(|item| raw_of(&item, depth + 1))
            .collect();
        return Raw::Array(items);
    }
    if type_id == AXValueGetTypeID()
        && let Some(raw) = ax_value(value)
    {
        return raw;
    }
    Raw::Unsupported(type_description(value))
}

/// A `CFNumber`, as an integer unless Core Foundation stores it as floating point.
fn number(value: &Owned) -> Raw<Element> {
    let pointer = value.as_ptr();
    // SAFETY: `value` is a live `CFNumber`.
    let float = unsafe { CFNumberIsFloatType(pointer) } != 0;
    if !float {
        let mut integer: i64 = 0;
        // SAFETY: `value` is a live `CFNumber` and `integer` an `i64`, which
        // `kCFNumberSInt64Type` writes.
        let exact = unsafe {
            CFNumberGetValue(pointer, NUMBER_SINT64, (&raw mut integer).cast::<c_void>())
        };
        if exact != 0 {
            return Raw::Number(NumberValue::Integer(integer));
        }
    }
    let mut double: f64 = 0.0;
    // SAFETY: `value` is a live `CFNumber` and `double` an `f64`, which `kCFNumberFloat64Type`
    // writes.
    unsafe { CFNumberGetValue(pointer, NUMBER_FLOAT64, (&raw mut double).cast::<c_void>()) };
    Raw::Number(NumberValue::Float(double))
}

/// A point, size, rect or range `AXValue`, or `None` for any other kind.
fn ax_value(value: &Owned) -> Option<Raw<Element>> {
    let pointer = value.as_ptr();
    // SAFETY: `value` is a live `AXValue`.
    let kind = unsafe { AXValueGetType(pointer) };
    // SAFETY (for each read below): `value` is a live `AXValue` of `kind`, and the out-pointer is
    // the C struct that kind holds.
    match kind {
        AX_VALUE_POINT => {
            let mut point = CGPoint::default();
            // SAFETY: as above, a `CGPoint` for `kAXValueCGPointType`.
            let read = unsafe { AXValueGetValue(pointer, kind, (&raw mut point).cast::<c_void>()) };
            (read != 0).then_some(Raw::Point {
                x: point.x,
                y: point.y,
            })
        }
        AX_VALUE_SIZE => {
            let mut size = CGSize::default();
            // SAFETY: as above, a `CGSize` for `kAXValueCGSizeType`.
            let read = unsafe { AXValueGetValue(pointer, kind, (&raw mut size).cast::<c_void>()) };
            (read != 0).then_some(Raw::Size {
                width: size.width,
                height: size.height,
            })
        }
        AX_VALUE_RECT => {
            let mut rect = CGRect::default();
            // SAFETY: as above, a `CGRect` for `kAXValueCGRectType`.
            let read = unsafe { AXValueGetValue(pointer, kind, (&raw mut rect).cast::<c_void>()) };
            (read != 0).then_some(Raw::Rect {
                x: rect.origin.x,
                y: rect.origin.y,
                width: rect.size.width,
                height: rect.size.height,
            })
        }
        AX_VALUE_RANGE => {
            let mut range = CFRange::default();
            // SAFETY: as above, a `CFRange` for `kAXValueCFRangeType`.
            let read = unsafe { AXValueGetValue(pointer, kind, (&raw mut range).cast::<c_void>()) };
            (read != 0).then_some(Raw::Range {
                location: i64::try_from(range.location).unwrap_or(i64::MAX),
                length: i64::try_from(range.length).unwrap_or(i64::MAX),
            })
        }
        _ => None,
    }
}
