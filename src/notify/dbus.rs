//! Minimal D-Bus client: just enough of the wire protocol to call
//! `org.freedesktop.Notifications.Notify` on the session bus.
//!
//! Spec: https://dbus.freedesktop.org/doc/dbus-specification.html
//!       https://specifications.freedesktop.org/notification-spec/latest/

use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

// Message types
const METHOD_CALL: u8 = 1;
const METHOD_RETURN: u8 = 2;
const ERROR: u8 = 3;
// Header fields
const FIELD_PATH: u8 = 1;
const FIELD_INTERFACE: u8 = 2;
const FIELD_MEMBER: u8 = 3;
const FIELD_ERROR_NAME: u8 = 4;
const FIELD_REPLY_SERIAL: u8 = 5;
const FIELD_DESTINATION: u8 = 6;
const FIELD_SIGNATURE: u8 = 8;

/// Little-endian D-Bus marshaller.
#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn align(&mut self, n: usize) {
        while !self.buf.len().is_multiple_of(n) {
            self.buf.push(0);
        }
    }
    fn byte(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.align(4);
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// STRING and OBJECT_PATH
    fn string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }
    fn signature(&mut self, s: &str) {
        self.byte(s.len() as u8);
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
    }
    /// ARRAY: length, padding to the element alignment, elements.
    fn array(&mut self, element_align: usize, write: impl FnOnce(&mut Self)) {
        self.u32(0);
        let len_pos = self.buf.len() - 4;
        self.align(element_align);
        let start = self.buf.len();
        write(self);
        let len = (self.buf.len() - start) as u32;
        self.buf[len_pos..len_pos + 4].copy_from_slice(&len.to_le_bytes());
    }
    /// Header field `(y v)` with a string-like value.
    fn field(&mut self, code: u8, sig: &str, value: &str) {
        self.align(8);
        self.byte(code);
        self.signature(sig);
        match sig {
            "g" => self.signature(value),
            _ => self.string(value),
        }
    }
}

struct Call<'a> {
    path: &'a str,
    interface: &'a str,
    member: &'a str,
    destination: &'a str,
    signature: &'a str,
    body: Vec<u8>,
}

fn encode_call(serial: u32, call: &Call) -> Vec<u8> {
    let mut w = Writer::default();
    w.byte(b'l');
    w.byte(METHOD_CALL);
    w.byte(0); // flags
    w.byte(1); // protocol version
    w.u32(call.body.len() as u32);
    w.u32(serial);
    w.array(8, |w| {
        w.field(FIELD_PATH, "o", call.path);
        w.field(FIELD_INTERFACE, "s", call.interface);
        w.field(FIELD_MEMBER, "s", call.member);
        w.field(FIELD_DESTINATION, "s", call.destination);
        if !call.signature.is_empty() {
            w.field(FIELD_SIGNATURE, "g", call.signature);
        }
    });
    w.align(8);
    w.buf.extend_from_slice(&call.body);
    w.buf
}

/// Body of `Notify(susssasa{sv}i)`.
fn notify_body(app: &str, summary: &str, body: &str) -> Vec<u8> {
    let mut w = Writer::default();
    w.string(app); // app_name
    w.u32(0); // replaces_id
    w.string(""); // app_icon
    w.string(summary);
    w.string(body);
    w.array(4, |_| {}); // actions: as
    w.array(8, |_| {}); // hints: a{sv}
    w.i32(-1); // expire_timeout: server default
    w.buf
}

/// Some servers render a subset of HTML in the body; package names must not inject markup.
fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A received message, reduced to what we need.
struct Reply {
    kind: u8,
    reply_serial: Option<u32>,
    error_name: Option<String>,
}

fn read_u32(b: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(b[pos..pos + 4].try_into().unwrap())
}

fn read_reply(stream: &mut impl Read) -> Result<Reply> {
    let mut fixed = [0u8; 16];
    stream.read_exact(&mut fixed)?;
    if fixed[0] != b'l' {
        bail!("big-endian replies are not supported");
    }
    let body_len = read_u32(&fixed, 4) as usize;
    let fields_len = read_u32(&fixed, 12) as usize;
    let header_rest = fields_len.div_ceil(8) * 8;
    let mut rest = vec![0u8; header_rest + body_len];
    stream.read_exact(&mut rest)?;

    // Fields start at absolute offset 16; positions below are absolute.
    let mut whole = fixed.to_vec();
    whole.extend_from_slice(&rest);
    let end = 16 + fields_len;
    let mut pos = 16;
    let mut reply = Reply {
        kind: fixed[1],
        reply_serial: None,
        error_name: None,
    };
    let align = |p: usize, n: usize| p.div_ceil(n) * n;
    while pos < end {
        pos = align(pos, 8);
        let code = whole[pos];
        let sig_len = whole[pos + 1] as usize;
        let sig = whole[pos + 2..pos + 2 + sig_len].to_vec();
        pos += 3 + sig_len;
        match sig.as_slice() {
            b"u" => {
                pos = align(pos, 4);
                if code == FIELD_REPLY_SERIAL {
                    reply.reply_serial = Some(read_u32(&whole, pos));
                }
                pos += 4;
            }
            b"s" | b"o" => {
                pos = align(pos, 4);
                let len = read_u32(&whole, pos) as usize;
                if code == FIELD_ERROR_NAME {
                    reply.error_name =
                        Some(String::from_utf8_lossy(&whole[pos + 4..pos + 4 + len]).into_owned());
                }
                pos += 4 + len + 1;
            }
            b"g" => {
                let len = whole[pos] as usize;
                pos += 1 + len + 1;
            }
            _ => break,
        }
    }
    Ok(reply)
}

fn session_bus() -> Result<UnixStream> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok().or_else(|| {
        // Fall back to the systemd default location.
        let uid = std::fs::metadata("/proc/self")
            .ok()
            .map(|m| std::os::unix::fs::MetadataExt::uid(&m))?;
        Some(format!("unix:path=/run/user/{uid}/bus"))
    });
    let address = address.context("no session bus (DBUS_SESSION_BUS_ADDRESS not set)")?;
    for entry in address.split(';') {
        let Some(params) = entry.strip_prefix("unix:") else {
            continue;
        };
        for kv in params.split(',') {
            if let Some(path) = kv.strip_prefix("path=")
                && let Ok(s) = UnixStream::connect(path)
            {
                return Ok(s);
            }
            #[cfg(any(target_os = "linux", target_os = "android"))]
            if let Some(name) = kv.strip_prefix("abstract=") {
                use std::os::linux::net::SocketAddrExt;
                let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
                if let Ok(s) = UnixStream::connect_addr(&addr) {
                    return Ok(s);
                }
            }
        }
    }
    bail!("cannot connect to session bus at {address}")
}

/// SASL EXTERNAL authentication with our uid.
fn authenticate(stream: &mut UnixStream) -> Result<()> {
    let uid = std::fs::metadata("/proc/self")
        .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
        .unwrap_or_else(|_| {
            // Not every Unix has /proc; the uid of our own temp file is the same.
            let f = std::env::temp_dir().join(format!("kliknload-uid-{}", std::process::id()));
            let _ = std::fs::write(&f, b"");
            let uid = std::fs::metadata(&f)
                .map(|m| std::os::unix::fs::MetadataExt::uid(&m))
                .unwrap_or(0);
            let _ = std::fs::remove_file(&f);
            uid
        });
    let hex: String = uid
        .to_string()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    stream.write_all(b"\0")?;
    stream.write_all(format!("AUTH EXTERNAL {hex}\r\n").as_bytes())?;
    let mut line = String::new();
    BufReader::new(&mut *stream).read_line(&mut line)?;
    if !line.starts_with("OK ") {
        bail!("D-Bus authentication failed: {}", line.trim());
    }
    stream.write_all(b"BEGIN\r\n")?;
    Ok(())
}

pub fn notify(app: &str, title: &str, message: &str) -> Result<()> {
    let mut stream = session_bus()?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    authenticate(&mut stream)?;

    let hello = Call {
        path: "/org/freedesktop/DBus",
        interface: "org.freedesktop.DBus",
        member: "Hello",
        destination: "org.freedesktop.DBus",
        signature: "",
        body: Vec::new(),
    };
    let notify = Call {
        path: "/org/freedesktop/Notifications",
        interface: "org.freedesktop.Notifications",
        member: "Notify",
        destination: "org.freedesktop.Notifications",
        signature: "susssasa{sv}i",
        body: notify_body(app, title, &escape_markup(message)),
    };
    stream.write_all(&encode_call(1, &hello))?;
    stream.write_all(&encode_call(2, &notify))?;

    // Wait for the answer to Notify (skipping the Hello reply and signals).
    for _ in 0..16 {
        let reply = read_reply(&mut stream)?;
        if reply.reply_serial != Some(2) {
            continue;
        }
        return match reply.kind {
            METHOD_RETURN => Ok(()),
            ERROR => bail!(
                "{}",
                reply.error_name.unwrap_or_else(|| "D-Bus error".into())
            ),
            other => bail!("unexpected D-Bus message type {other}"),
        };
    }
    bail!("no answer from the notification service")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marshals_hello_like_libdbus() {
        // Reference bytes of `Hello` as sent by libdbus (serial 1).
        let hello = Call {
            path: "/org/freedesktop/DBus",
            interface: "org.freedesktop.DBus",
            member: "Hello",
            destination: "org.freedesktop.DBus",
            signature: "",
            body: Vec::new(),
        };
        let m = encode_call(1, &hello);
        assert_eq!(&m[..4], b"l\x01\x00\x01");
        assert_eq!(read_u32(&m, 4), 0); // body length
        assert_eq!(read_u32(&m, 8), 1); // serial
        assert_eq!(m.len() % 8, 0);
        let fields_len = read_u32(&m, 12) as usize;
        assert_eq!(m.len(), 16 + fields_len.div_ceil(8) * 8);
    }

    #[test]
    fn notify_body_layout() {
        let body = notify_body("a", "T", "B");
        // "a": len 1 + 'a' + NUL, padded; replaces_id at offset 8
        assert_eq!(read_u32(&body, 0), 1);
        assert_eq!(read_u32(&body, 8), 0);
        // expire_timeout is the last int32
        assert_eq!(&body[body.len() - 4..], &(-1i32).to_le_bytes());
    }

    #[test]
    fn parses_error_reply() {
        // Build an error reply with reply_serial 2 and an error name.
        let mut w = Writer::default();
        w.byte(b'l');
        w.byte(ERROR);
        w.byte(0);
        w.byte(1);
        w.u32(0);
        w.u32(7);
        w.array(8, |w| {
            w.field(
                FIELD_ERROR_NAME,
                "s",
                "org.freedesktop.DBus.Error.ServiceUnknown",
            );
            w.align(8);
            w.byte(FIELD_REPLY_SERIAL);
            w.signature("u");
            w.u32(2);
        });
        w.align(8);
        let reply = read_reply(&mut w.buf.as_slice()).unwrap();
        assert_eq!(reply.kind, ERROR);
        assert_eq!(reply.reply_serial, Some(2));
        assert_eq!(
            reply.error_name.as_deref(),
            Some("org.freedesktop.DBus.Error.ServiceUnknown")
        );
    }
}
