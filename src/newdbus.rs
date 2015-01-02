use super::ffi;
use super::MessageItem;

use std;
use std::ptr;

#[allow(missing_copy_implementations)]
pub struct Connection(*mut ffi::DBusConnection);

impl Connection {
    /// Creates a new private session on the session bus.
    ///
    /// # Example
    ///
    /// ```
    /// use dbus::newdbus::Connection;
    ///
    /// let conn = match Connection::new() {
    ///     Ok(conn) => conn,
    ///     Err(e) => panic!("failed to create connection: {}", e),
    /// };
    pub fn new() -> Result<Connection, super::Error> {
        Connection::new_for_type(super::BusType::Session)
    }

    /// Creates a new private session on the given bus.
    pub fn new_for_type(bus: super::BusType) -> Result<Connection, super::Error> {
        let mut e = super::Error::empty();
        let c = unsafe { ffi::dbus_bus_get_private(bus, e.get_mut()) };
        if c == ptr::null_mut() {
            return Err(e);
        }

        /* No, we don't want our app to suddenly quit if dbus goes down */
        unsafe { ffi::dbus_connection_set_exit_on_disconnect(c, 0) };

        Ok(Connection(c))
    }

    /// Utility method for sending a message and synchronously waiting for its response.
    unsafe fn send_sync(&self, msg: *mut ffi::DBusMessage)
                        -> Result<(*mut ffi::DBusMessage, super::MessageType), super::Error> {
        let mut e = super::Error::empty();
        // -1 tells DBus to use the default timeout.
        let resp = ffi::dbus_connection_send_with_reply_and_block(self.0, msg, -1, e.get_mut());
        if resp != ptr::null_mut() {
            Ok((resp, std::mem::transmute(ffi::dbus_message_get_type(resp))))
        } else {
            Err(e)
        }
    }

    /// Call a method on this bus and synchronously wait for its response.
    ///
    /// * `destination` may be an empty string to indicate that it should be peer-to-peer.
    /// * `iface` may be an empty string if the method name is unique enough to not require it.
    ///
    /// `path` and `method` must be filled in.
    ///
    /// # Example
    ///
    /// ```
    /// use dbus::newdbus::Connection;
    ///
    /// let conn = match Connection::new() {
    ///     Ok(conn) => conn,
    ///     Err(e) => panic!("failed to create connection: {}", e),
    /// };
    ///
    /// match conn.call_method_sync(
    ///     "org.mpris.MediaPlayer2",
    ///     "/org/mpris/MediaPlayer2",
    ///     "org.mpris.MediaPlayer2.Player",
    ///     "Play",
    ///     &[], // arguments slice
    /// ) {
    ///     Ok(resp) => { /* handle the response */ },
    ///     Err(e) => { /* something went wrong */ },
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the underlying DBus method returns NULL, which only happens if the system
    /// has run out of memory.
    pub fn call_method_sync<D, P, I, M>(&self, destination: D, path: P, iface: I, method: M, args: &[MessageItem])
                                       -> Result<MethodReturn, super::Error>
        where D: ToCStr, P: ToCStr, I: ToCStr, M: ToCStr
    {
        let msg = MethodCall::new(destination, path, iface, method);
        msg.append_items(args);
        match unsafe { self.send_sync(msg.0) } {
            Ok((resp, typ)) => match typ {
                super::MessageType::MethodReturn => Ok(MethodReturn(resp)),
                _ => panic!("method call received non-method-return value in response: {}", typ),
            },
            Err(e) => Err(e),
        }
    }

    pub fn stub<D, P>(&mut self, destination: D, path: P) -> Object
        where D: ToString, P: ToString
    {
        Object::new(self, destination, path)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            ffi::dbus_connection_close(self.0);
            ffi::dbus_connection_unref(self.0);
        }
    }
}

pub struct Object {
    conn: *mut Connection,
    destination: String,
    path: String,
}

impl Object {
    /// Create a new DBus object stub.
    ///
    /// Object stubs are useful for defining a reusable endpoint, avoiding
    /// the need to specify the destination and path every time.
    ///
    /// # Example
    ///
    /// ```
    /// use dbus::newdbus::Connection;
    ///
    /// let mut conn = match Connection::new() {
    ///     Ok(conn) => conn,
    ///     Err(e) => panic!("failed to create connection: {}", e),
    /// };
    ///
    /// let media_player = conn.stub("org.mpris.MediaPlayer2", "/org/mpris/MediaPlayer2");
    ///
    /// match media_player.call_full("org.mpris.MediaPlayer2.Player", "Play", &[]) {
    ///     Ok(resp) => { /* handle the response */ },
    ///     Err(e) => { /* something went wrong */ },
    /// }
    ///
    /// // shorthand if the method name is unambiguous
    /// match media_player.call("Pause", &[]) {
    ///     Ok(resp) => { /* handle the response */ },
    ///     Err(e) => { /* something went wrong */ },
    /// }
    /// ```
    pub fn new<D, P>(conn: &mut Connection, destination: D, path: P) -> Object
        where D: ToString, P: ToString
    {
        Object{
            conn: conn as *mut Connection,
            destination: destination.to_string(),
            path: path.to_string(),
        }
    }

    pub fn call_full<I, M>(&self, iface: I, method: M, args: &[MessageItem]) -> Result<MethodReturn, super::Error>
        where I: ToCStr, M: ToCStr
    {
        unsafe {
            (*self.conn).call_method_sync(self.destination.as_slice(), self.path.as_slice(), iface, method, args)
        }
    }

    pub fn call<M>(&self, method: M, args: &[MessageItem]) -> Result<MethodReturn, super::Error>
        where M: ToCStr
    {
        self.call_full("", method, args)
    }
}

/// Macro for defining each of the message types and providing them
/// with an implementation of `Message` that utilizes helper functions.
macro_rules! define_message_types {
    ($($i:ident),+) => {$(
        #[allow(missing_copy_implementations)]
        pub struct $i(*mut ffi::DBusMessage);

        // NOTE: it would be nice if this could hook into #[deriving]
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
    MethodReturn,
    Error
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
    pub fn new<D, P, I, M>(destination: D, path: P, iface: I, method: M) -> MethodCall
        where D: ToCStr, P: ToCStr, I: ToCStr, M: ToCStr
    {
        super::init_dbus();

        let destination = destination.to_c_str();
        let path = path.to_c_str();
        let iface = iface.to_c_str();
        let method = method.to_c_str();

        MethodCall(check_memory!(unsafe {
            ffi::dbus_message_new_method_call(
                if destination.is_empty() { ptr::null() } else { destination.as_ptr() },
                path.as_ptr(),
                if iface.is_empty() { ptr::null() } else { iface.as_ptr() },
                method.as_ptr(),
            )
        }))
    }

    /// Create a new response for this call.
    pub fn new_return(&self) -> MethodReturn {
        MethodReturn(check_memory!(unsafe { ffi::dbus_message_new_method_return(self.0) }))
    }

    /// Create a new error in response to this call.
    ///
    /// If `name` is empty, then the string `"org.freedesktop.DBus.Error.Failed"` will
    /// be used instead.
    pub fn new_error<N, M>(&self, name: N, message: M) -> Error
        where N: ToCStr, M: ToCStr
    {
        Error::new(self.0, name, message)
    }

    /// Create a new response for this call and populate it with the provided messages.
    pub fn respond_with(&self, v: &[MessageItem]) -> MethodReturn {
        let response = self.new_return();
        response.append_items(v);
        response
    }
}

impl Error {
    /// Helper for constructing error messages.
    fn new<N, M>(reply_to: *mut ffi::DBusMessage, name: N, message: M) -> Error
        where N: ToCStr, M: ToCStr
    {
        let mut name = name.to_c_str();
        let message = message.to_c_str();

        if name.is_empty() {
            name = "org.freedesktop.DBus.Error.Failed".to_c_str();
        }

        Error(check_memory!(unsafe {
            ffi::dbus_message_new_error(
                reply_to,
                name.as_ptr(),
                message.as_ptr(),
            )
        }))
    }
}
