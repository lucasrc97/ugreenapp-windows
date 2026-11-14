//! SDP service lookup through `WSALookupService*` (namespace `NS_BTH`).

use std::mem::size_of;

use windows_sys::Win32::Devices::Bluetooth::{NS_BTH, SOCKADDR_BTH};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Networking::WinSock::{
    CSADDR_INFO, LUP_FLUSHCACHE, LUP_RETURN_ADDR, LUP_RETURN_NAME, WSA_E_NO_MORE, WSAEFAULT,
    WSAENOMORE, WSALookupServiceBeginW, WSALookupServiceEnd, WSALookupServiceNextW, WSAQUERYSETW,
    WSASERVICE_NOT_FOUND,
};
use windows_sys::core::GUID;

use super::socket::{WsaError, last_wsa_error};
use crate::transport::bth_addr_to_mac;

/// 16-bit Bluetooth UUIDs used as SDP search patterns.
pub(super) const UUID_RFCOMM: u16 = 0x0003;
pub(super) const UUID_SERIAL_PORT: u16 = 0x1101;
pub(super) const UUID_HEADSET: u16 = 0x1108;
pub(super) const UUID_HANDSFREE: u16 = 0x111E;

pub(super) struct SdpService {
    pub(super) name: String,
    pub(super) channel: u8,
}

/// Expands a 16-bit UUID with the Bluetooth base UUID
/// (`0000xxxx-0000-1000-8000-00805F9B34FB`).
fn bluetooth_uuid(short: u16) -> GUID {
    GUID::from_u128(0x0000_0000_0000_1000_8000_0080_5F9B_34FB | (u128::from(short) << 96))
}

/// Lists the RFCOMM services of `bth_addr` whose SDP record contains `uuid`.
/// `flush_cache` forces an over-the-air query instead of Windows' cached
/// records. Requires a successful `ensure_winsock`.
pub(super) fn lookup(
    bth_addr: u64,
    uuid: u16,
    flush_cache: bool,
) -> Result<Vec<SdpService>, WsaError> {
    let mut flags = LUP_RETURN_NAME | LUP_RETURN_ADDR;
    if flush_cache {
        flags |= LUP_FLUSHCACHE;
    }

    let lookup = match LookupHandle::begin(bth_addr, uuid, flags) {
        Ok(lookup) => lookup,
        Err(WSASERVICE_NOT_FOUND) => return Ok(Vec::new()),
        Err(code) => return Err(code),
    };

    let mut services = Vec::new();
    // u64 elements give the 8-byte alignment WSAQUERYSETW needs.
    let mut buffer = vec![0u64; 512];
    loop {
        match lookup.next(flags, &mut buffer) {
            Ok(()) => services.extend(parse_result(&buffer)),
            Err(NextError::Done) => break,
            Err(NextError::BufferTooSmall(bytes)) => buffer.resize(bytes.div_ceil(8), 0),
            Err(NextError::Failed(code)) => return Err(code),
        }
    }
    Ok(services)
}

enum NextError {
    Done,
    BufferTooSmall(usize),
    Failed(WsaError),
}

/// An open lookup, ended on drop.
struct LookupHandle(HANDLE);

impl LookupHandle {
    fn begin(bth_addr: u64, uuid: u16, flags: u32) -> Result<Self, WsaError> {
        let mut class_id = bluetooth_uuid(uuid);
        // The device to query, as the NUL-terminated string "(AA:BB:CC:DD:EE:FF)".
        let mut context = format!("({})", bth_addr_to_mac(bth_addr))
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>();

        // SAFETY: WSAQUERYSETW holds only integers and raw pointers, for which
        // all-zero (0 / null) is a valid value meaning "not specified".
        let mut query: WSAQUERYSETW = unsafe { std::mem::zeroed() };
        query.dwSize = size_of::<WSAQUERYSETW>() as u32;
        query.dwNameSpace = NS_BTH;
        query.lpServiceClassId = &mut class_id;
        query.lpszContext = context.as_mut_ptr();

        let mut handle: HANDLE = std::ptr::null_mut();
        // SAFETY: `query` is a correctly sized WSAQUERYSETW whose two non-null
        // pointers refer to `class_id` and the NUL-terminated `context`, both
        // alive until after the call returns; Winsock copies what it needs.
        // `handle` is a live out-parameter.
        let rc = unsafe { WSALookupServiceBeginW(&query, flags, &mut handle) };
        if rc != 0 {
            return Err(last_wsa_error());
        }
        Ok(Self(handle))
    }

    /// Writes the next result set to the start of `buffer`.
    fn next(&self, flags: u32, buffer: &mut [u64]) -> Result<(), NextError> {
        let capacity = std::mem::size_of_val(buffer);
        let mut len = u32::try_from(capacity).unwrap_or(u32::MAX);
        // SAFETY: `self.0` is a handle from a successful Begin, not yet ended.
        // `buffer` is writable, 8-byte aligned, and `len` does not exceed its
        // size in bytes, so Winsock writes only inside it.
        let rc =
            unsafe { WSALookupServiceNextW(self.0, flags, &mut len, buffer.as_mut_ptr().cast()) };
        if rc == 0 {
            return Ok(());
        }
        match last_wsa_error() {
            WSA_E_NO_MORE | WSAENOMORE => Err(NextError::Done),
            WSAEFAULT if len as usize > capacity => Err(NextError::BufferTooSmall(len as usize)),
            code => Err(NextError::Failed(code)),
        }
    }
}

impl Drop for LookupHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful Begin and is ended once.
        unsafe { WSALookupServiceEnd(self.0) };
    }
}

/// Extracts name and RFCOMM channel(s) from a result set that
/// `LookupHandle::next` just wrote to the start of `buffer`.
fn parse_result(buffer: &[u64]) -> Vec<SdpService> {
    debug_assert!(std::mem::size_of_val(buffer) >= size_of::<WSAQUERYSETW>());
    // SAFETY: the caller passes a buffer that a successful
    // WSALookupServiceNextW filled, so it starts with an initialised
    // WSAQUERYSETW; the u64 backing store satisfies its alignment and the
    // shared borrow of `buffer` keeps it alive and unmodified.
    let result = unsafe { &*buffer.as_ptr().cast::<WSAQUERYSETW>() };

    let name = if result.lpszServiceInstanceName.is_null() {
        String::new()
    } else {
        // SAFETY: per the API contract a non-null name points to a
        // NUL-terminated UTF-16 string stored inside the same result buffer.
        unsafe { wide_cstr_to_string(result.lpszServiceInstanceName) }
    };

    let mut services = Vec::new();
    if result.lpcsaBuffer.is_null() {
        return services;
    }
    for index in 0..result.dwNumberOfCsAddrs as usize {
        // SAFETY: `lpcsaBuffer` points to `dwNumberOfCsAddrs` CSADDR_INFO
        // entries inside the result buffer and `index` is below that count.
        // `read_unaligned` makes no assumption about their alignment.
        let info: CSADDR_INFO = unsafe { result.lpcsaBuffer.add(index).read_unaligned() };
        let remote = info.RemoteAddr;
        if remote.lpSockaddr.is_null()
            || (remote.iSockaddrLength as usize) < size_of::<SOCKADDR_BTH>()
        {
            continue;
        }
        // SAFETY: the pointer is non-null and the provider reported at least
        // size_of::<SOCKADDR_BTH>() readable bytes behind it; for NS_BTH the
        // address is a SOCKADDR_BTH. It is packed, hence `read_unaligned`.
        let address = unsafe { remote.lpSockaddr.cast::<SOCKADDR_BTH>().read_unaligned() };
        let port = address.port;
        if let Ok(channel @ 1..=30) = u8::try_from(port) {
            services.push(SdpService {
                name: name.clone(),
                channel,
            });
        }
    }
    services
}

/// # Safety
/// `pointer` must be non-null and point to a readable, NUL-terminated UTF-16
/// string that stays valid for the duration of the call.
unsafe fn wide_cstr_to_string(pointer: *const u16) -> String {
    let mut len = 0;
    // SAFETY: the caller guarantees a NUL terminator, so every offset up to
    // and including it is readable.
    while unsafe { pointer.add(len).read_unaligned() } != 0 {
        len += 1;
    }
    let units = (0..len)
        // SAFETY: `offset < len`, inside the string measured above.
        .map(|offset| unsafe { pointer.add(offset).read_unaligned() })
        .collect::<Vec<u16>>();
    String::from_utf16_lossy(&units)
}
