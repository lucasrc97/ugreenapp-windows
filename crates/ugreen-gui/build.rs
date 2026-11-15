//! Embeds `assets/icon.ico` in the Windows executable, so Explorer, the
//! taskbar and shortcuts show it.
//!
//! The icon is written as a compiled resource file (`.res`) by hand and
//! handed to the linker, so no resource compiler (`rc.exe`, `llvm-rc`) is
//! needed, natively or when cross-compiling. Only the MSVC linkers
//! (`link.exe`, `lld-link`) take a `.res` file; other targets get no icon.

use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Movable, pure, discardable: what resource compilers put on icons.
const MEMORY_FLAGS: u16 = 0x1030;
const LANGUAGE_NEUTRAL: u16 = 0;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let target = |name: &str| std::env::var(name).unwrap_or_default();
    if target("CARGO_CFG_TARGET_OS") != "windows" || target("CARGO_CFG_TARGET_ENV") != "msvc" {
        return;
    }
    let ico = std::fs::read("assets/icon.ico").expect("assets/icon.ico is readable");
    let res = icon_resources(&ico).expect("assets/icon.ico is a valid icon file");
    let path = PathBuf::from(target("OUT_DIR")).join("icon.res");
    std::fs::write(&path, res).expect("the resource file can be written");
    println!("cargo:rustc-link-arg-bins={}", path.display());
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// One resource: a 32-byte header (type and name given as numbers), then the
/// data padded to a multiple of four bytes.
fn push_resource(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    res.extend((data.len() as u32).to_le_bytes());
    res.extend(32u32.to_le_bytes());
    for id in [kind, name] {
        res.extend(0xFFFFu16.to_le_bytes());
        res.extend(id.to_le_bytes());
    }
    res.extend(0u32.to_le_bytes()); // data version
    res.extend(MEMORY_FLAGS.to_le_bytes());
    res.extend(LANGUAGE_NEUTRAL.to_le_bytes());
    res.extend(0u32.to_le_bytes()); // version
    res.extend(0u32.to_le_bytes()); // characteristics
    res.extend(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

/// Turns an `.ico` file into a `.res` file: one icon resource per image and
/// the group (directory) that lists them, which is what Windows looks up.
fn icon_resources(ico: &[u8]) -> Option<Vec<u8>> {
    if u16_at(ico, 0)? != 0 || u16_at(ico, 2)? != 1 {
        return None;
    }
    let count = u16_at(ico, 4)?;
    // Every `.res` file starts with an empty resource.
    let mut res = Vec::new();
    push_resource(&mut res, 0, 0, &[]);
    res[20..24].fill(0); // the marker carries no memory flags

    let mut group = Vec::new();
    group.extend(0u16.to_le_bytes());
    group.extend(1u16.to_le_bytes());
    group.extend(count.to_le_bytes());
    for index in 0..count {
        // File directory entry: 12 bytes of description, size, file offset.
        let entry = 6 + 16 * usize::from(index);
        let size = u32_at(ico, entry + 8)? as usize;
        let offset = u32_at(ico, entry + 12)? as usize;
        let image = ico.get(offset..offset.checked_add(size)?)?;
        let id = index + 1;
        push_resource(&mut res, RT_ICON, id, image);
        // Group entry: the same description and size, then the icon's id.
        group.extend(ico.get(entry..entry + 12)?);
        group.extend(id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, 1, &group);
    Some(res)
}
