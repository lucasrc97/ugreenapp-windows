//! Winsock RFCOMM client socket. Every `unsafe` block lives in a small
//! function whose `SAFETY` comment states the invariant it relies on.

use std::mem::size_of;
use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Devices::Bluetooth::{AF_BTH, BTHPROTO_RFCOMM, SOCKADDR_BTH};
use windows_sys::Win32::Networking::WinSock::{
    FD_SET, INVALID_SOCKET, SO_RCVTIMEO, SO_SNDTIMEO, SOCK_STREAM, SOCKADDR, SOCKET, SOCKET_ERROR,
    SOL_SOCKET, TIMEVAL, WSADATA, WSAGetLastError, WSAStartup, closesocket, connect, recv, select,
    send, setsockopt, socket,
};
use windows_sys::core::GUID;

/// A Winsock error code (`WSAGetLastError`).
pub(super) type WsaError = i32;

/// Initialises Winsock 2.2 once per process. It is never torn down: the
/// process exit does that, and it keeps every socket valid for its lifetime.
pub(super) fn ensure_winsock() -> Result<(), WsaError> {
    static STARTUP: OnceLock<WsaError> = OnceLock::new();
    match *STARTUP.get_or_init(wsa_startup) {
        0 => Ok(()),
        code => Err(code),
    }
}

fn wsa_startup() -> WsaError {
    const WINSOCK_2_2: u16 = 0x0202;
    // SAFETY: WSADATA is plain old data (integers, byte arrays and a pointer
    // Winsock does not read), so the all-zero pattern is a valid value.
    let mut data: WSADATA = unsafe { std::mem::zeroed() };
    // SAFETY: `data` is a live, writable WSADATA for the duration of the call.
    unsafe { WSAStartup(WINSOCK_2_2, &mut data) }
}

pub(super) fn last_wsa_error() -> WsaError {
    // SAFETY: no arguments and no preconditions; reads thread-local state.
    unsafe { WSAGetLastError() }
}

/// An owned `AF_BTH`/`SOCK_STREAM`/`BTHPROTO_RFCOMM` socket, closed on drop.
pub(super) struct RfcommSocket(SOCKET);

impl RfcommSocket {
    /// Requires a successful [`ensure_winsock`].
    pub(super) fn open() -> Result<Self, WsaError> {
        // SAFETY: plain value arguments; Winsock is initialised by the caller.
        let raw = unsafe { socket(AF_BTH.into(), SOCK_STREAM, BTHPROTO_RFCOMM as i32) };
        if raw == INVALID_SOCKET {
            return Err(last_wsa_error());
        }
        Ok(Self(raw))
    }

    /// Sets `SO_SNDTIMEO` and `SO_RCVTIMEO`.
    pub(super) fn set_timeouts(&self, timeout: Duration) -> Result<(), WsaError> {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        self.set_u32_option(SO_SNDTIMEO, millis)?;
        self.set_u32_option(SO_RCVTIMEO, millis)
    }

    fn set_u32_option(&self, option: i32, value: u32) -> Result<(), WsaError> {
        // SAFETY: `self.0` is an open socket we own. `optval` points to a live
        // u32 and `optlen` is exactly its size; on Windows both timeout
        // options take a DWORD of milliseconds.
        let rc = unsafe {
            setsockopt(
                self.0,
                SOL_SOCKET,
                option,
                (&raw const value).cast(),
                size_of::<u32>() as i32,
            )
        };
        if rc == SOCKET_ERROR {
            return Err(last_wsa_error());
        }
        Ok(())
    }

    /// Connects to RFCOMM `channel` (1..=30) of the device at `bth_addr`.
    pub(super) fn connect(&self, bth_addr: u64, channel: u8) -> Result<(), WsaError> {
        let address = SOCKADDR_BTH {
            addressFamily: AF_BTH,
            btAddr: bth_addr,
            // A null service class with an explicit port means "this channel,
            // no SDP lookup".
            serviceClassId: GUID::from_u128(0),
            port: u32::from(channel),
        };
        // SAFETY: `self.0` is an open socket we own. `address` is a live
        // SOCKADDR_BTH and `namelen` is exactly its size, so Winsock reads
        // only that struct; AF_BTH tells it how to interpret the bytes.
        let rc = unsafe {
            connect(
                self.0,
                (&raw const address).cast::<SOCKADDR>(),
                size_of::<SOCKADDR_BTH>() as i32,
            )
        };
        if rc == SOCKET_ERROR {
            return Err(last_wsa_error());
        }
        Ok(())
    }

    /// Sends some prefix of `bytes`, returning how many bytes were accepted.
    pub(super) fn send(&self, bytes: &[u8]) -> Result<usize, WsaError> {
        let len = i32::try_from(bytes.len()).unwrap_or(i32::MAX);
        // SAFETY: `self.0` is an open socket we own. The pointer and `len`
        // describe (a prefix of) the live slice `bytes`, which is only read.
        let sent = unsafe { send(self.0, bytes.as_ptr(), len, 0) };
        if sent == SOCKET_ERROR {
            return Err(last_wsa_error());
        }
        Ok(sent as usize)
    }

    /// Waits up to `timeout` for the socket to become readable.
    pub(super) fn wait_readable(&self, timeout: Duration) -> Result<bool, WsaError> {
        let mut sockets = [0; 64];
        sockets[0] = self.0;
        let mut read_set = FD_SET {
            fd_count: 1,
            fd_array: sockets,
        };
        let timeout = TIMEVAL {
            tv_sec: i32::try_from(timeout.as_secs()).unwrap_or(i32::MAX),
            tv_usec: timeout.subsec_micros() as i32,
        };
        // SAFETY: `read_set` and `timeout` are live for the call; `fd_count`
        // (1) does not exceed the array and its one entry is our open socket.
        // The write/except sets may be null. `nfds` is ignored on Windows.
        let ready = unsafe {
            select(
                0,
                &mut read_set,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &timeout,
            )
        };
        match ready {
            SOCKET_ERROR => Err(last_wsa_error()),
            count => Ok(count > 0),
        }
    }

    /// Receives into `buffer`; `Ok(0)` means the peer closed the connection.
    pub(super) fn recv(&self, buffer: &mut [u8]) -> Result<usize, WsaError> {
        let len = i32::try_from(buffer.len()).unwrap_or(i32::MAX);
        // SAFETY: `self.0` is an open socket we own. The pointer and `len`
        // describe (a prefix of) the live, exclusively borrowed `buffer`.
        let received = unsafe { recv(self.0, buffer.as_mut_ptr(), len, 0) };
        if received == SOCKET_ERROR {
            return Err(last_wsa_error());
        }
        Ok(received as usize)
    }
}

impl Drop for RfcommSocket {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful `socket()` and is closed
        // exactly once, here; no copy of the handle outlives `self`.
        unsafe { closesocket(self.0) };
    }
}
