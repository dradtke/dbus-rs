use super::ffi;
use super::MessageItem;

use std::ptr;

macro_rules! define_message_types {
    ($($i:ident),+) => {$(
        #[allow(missing_copy_implementations)]
        pub struct $i(*mut ffi::DBusMessage);

        // TODO: it would be nice if this could hook into #[deriving]
        impl Message for $i {
            fn get_items(&self) -> Vec<MessageItem> { get_items(self.0) }
            fn append_items(&self, v: &[MessageItem]) { append_items(self.0, v) }
        }
    )+}
}

/// Utility macro that panics on a null value. It should only be used
/// when calling methods that are guaranteed to return NULL if and
/// only if the system ran out of memory, which is true of many DBus
/// functions.
macro_rules! check_memory {
    ($e:expr) => (
        match $e {
            p if p == ptr::null_mut() => panic!("out of memory!"),
            p => p,
        }
    )
}

define_message_types! {
    MethodCall,
    MethodReturn
}

pub trait Message {
    fn get_items(&self) -> Vec<MessageItem>;
    fn append_items(&self, v: &[MessageItem]);
}

fn get_items(ptr: *mut ffi::DBusMessage) -> Vec<MessageItem> {
    let mut i = super::new_dbus_message_iter();
    match unsafe { ffi::dbus_message_iter_init(ptr, &mut i) } {
        0 => Vec::new(),
        _ => MessageItem::from_iter(&mut i)
    }
}

fn append_items(ptr: *mut ffi::DBusMessage, v: &[MessageItem]) {
    let mut i = super::new_dbus_message_iter();
    unsafe { ffi::dbus_message_iter_init_append(ptr, &mut i) };
    MessageItem::copy_to_iter(&mut i, v);
}

impl MethodCall {
    /// Create a new method call.
    ///
    /// * `destination` may be an empty string to indicate that it should be peer-to-peer.
    /// * `iface` may be an empty string if the method name is unique enough to not require it.
    ///
    /// `path` and `method` must be filled in.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use dbus::message::MethodCall;
    ///
    /// let m = MethodCall::new(
    ///     "org.freedesktop.PolicyKit1",
    ///     "/org/freedesktop/PolicyKit1/Authority",
    ///     "org.freedesktop.PolicyKit1.Authority",
    ///     "BackendVersion",
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the underlying DBus method returns NULL, which only happens if the system
    /// has run out of memory.
    pub fn new<D, P, I, M>(destination: D, path: P, iface: I, method: M) -> MethodCall
        where D: Str, P: Str, I: Str, M: Str
    {
        super::init_dbus();

        let destination = destination.as_slice();
        let path = path.as_slice();
        let iface = iface.as_slice();
        let method = method.as_slice();

        MethodCall(check_memory!(unsafe {
            ffi::dbus_message_new_method_call(
                match destination.len() { 0 => ptr::null(), _ => destination.to_c_str().as_ptr() },
                path.to_c_str().as_ptr(),
                match iface.len() { 0 => ptr::null(), _ => iface.to_c_str().as_ptr() },
                method.to_c_str().as_ptr(),
            )
        }))
    }

    /// Create a new response for this call.
    pub fn new_response(&self) -> MethodReturn {
        MethodReturn(check_memory!(unsafe { ffi::dbus_message_new_method_return(self.0) }))
    }

    /// Create a new response for this call and populate it with the provided messages.
    pub fn respond_with(&self, v: &[MessageItem]) -> MethodReturn {
        let response = self.new_response();
        response.append_items(v);
        response
    }
}
