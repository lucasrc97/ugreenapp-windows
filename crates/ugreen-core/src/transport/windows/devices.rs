//! Local radio and paired-device enumeration (`bluetoothapis.dll`).

use std::mem::size_of;

use windows_sys::Win32::Devices::Bluetooth::{
    BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BLUETOOTH_FIND_RADIO_PARAMS,
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindFirstRadio,
    BluetoothFindNextDevice, BluetoothFindRadioClose,
};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_ITEMS, GetLastError, HANDLE};

pub(super) struct PairedDevice {
    pub(super) address: u64,
    pub(super) name: String,
    pub(super) connected: bool,
}

/// Whether Windows exposes at least one Bluetooth radio.
pub(super) fn radio_present() -> bool {
    let params = BLUETOOTH_FIND_RADIO_PARAMS {
        dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
    };
    let mut radio: HANDLE = std::ptr::null_mut();
    // SAFETY: `params` has its size field set as the API requires and `radio`
    // is a live out-parameter.
    let find = unsafe { BluetoothFindFirstRadio(&params, &mut radio) };
    if find.is_null() {
        return false;
    }
    // SAFETY: a non-null `find` means the call succeeded, so `radio` and
    // `find` are valid handles that we own and release exactly once.
    unsafe {
        CloseHandle(radio);
        BluetoothFindRadioClose(find);
    }
    true
}

/// Devices Windows has paired (authenticated), without starting an inquiry.
/// `Err` carries the Win32 error code.
pub(super) fn paired_devices() -> Result<Vec<PairedDevice>, u32> {
    let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: 1,
        fReturnRemembered: 0,
        fReturnUnknown: 0,
        fReturnConnected: 0,
        fIssueInquiry: 0,
        cTimeoutMultiplier: 0,
        hRadio: std::ptr::null_mut(), // all local radios
    };
    let mut info = empty_device_info();

    // SAFETY: both structs have their size fields set as the API requires and
    // are live for the call; `info` is writable.
    let find = unsafe { BluetoothFindFirstDevice(&params, &mut info) };
    if find.is_null() {
        // SAFETY: no preconditions; reads the calling thread's last error.
        return match unsafe { GetLastError() } {
            ERROR_NO_MORE_ITEMS => Ok(Vec::new()),
            code => Err(code),
        };
    }

    let mut devices = Vec::new();
    loop {
        if info.fAuthenticated != 0 {
            devices.push(to_paired_device(&info));
        }
        info = empty_device_info();
        // SAFETY: `find` is the live handle returned above and `info` is a
        // writable BLUETOOTH_DEVICE_INFO with its size field set.
        if unsafe { BluetoothFindNextDevice(find, &mut info) } == 0 {
            break;
        }
    }
    // SAFETY: `find` is a valid find handle, closed exactly once.
    unsafe { BluetoothFindDeviceClose(find) };
    Ok(devices)
}

fn empty_device_info() -> BLUETOOTH_DEVICE_INFO {
    // SAFETY: BLUETOOTH_DEVICE_INFO is plain old data (integers, a union of
    // integers and arrays of integers), so all-zero is a valid value.
    let mut info: BLUETOOTH_DEVICE_INFO = unsafe { std::mem::zeroed() };
    info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
    info
}

fn to_paired_device(info: &BLUETOOTH_DEVICE_INFO) -> PairedDevice {
    // SAFETY: both union members are integer data covering the same bytes, so
    // reading `ullLong` is valid whichever member the API wrote.
    let address = unsafe { info.Address.Anonymous.ullLong } & 0x0000_FFFF_FFFF_FFFF;
    let name_len = info
        .szName
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.szName.len());
    PairedDevice {
        address,
        name: String::from_utf16_lossy(&info.szName[..name_len]),
        connected: info.fConnected != 0,
    }
}
