//! `rsc devices` — lists real run targets across platforms, Flutter-`flutter
//! devices`-style. Each entry's `id` is exactly what `rsc run --device <id>`
//! consumes (an iOS simulator UDID, an Android adb serial) — this command
//! and `rsc run`'s `--device` handling are two views of the same data, not
//! independently maintained.

use std::process::Command;

pub struct Device {
    pub platform: &'static str,
    pub name: String,
    pub id: String,
    pub status: String,
}

pub struct DevicesOptions;

impl DevicesOptions {
    pub fn from_args(args: &[String]) -> Result<Self, String> {
        if args.iter().any(|a| a == "--help" || a == "-h") {
            print_help();
            std::process::exit(0);
        }
        Ok(Self)
    }
}

pub fn print_help() {
    println!("rsc devices — list available run targets across platforms");
    println!();
    println!("USAGE:");
    println!("  rsc devices");
    println!();
    println!("Lists this host (macOS desktop), iOS simulators (via `xcrun simctl`), and");
    println!("connected Android devices/emulators (via `adb`). The id column is exactly");
    println!("what `rsc run --device <id>` expects — copy it directly.");
}

pub fn run() {
    let devices = list_devices();
    if devices.is_empty() {
        println!("No devices found.");
        return;
    }
    let name_w = devices.iter().map(|d| d.name.len()).max().unwrap_or(4).max(4);
    let id_w = devices.iter().map(|d| d.id.len()).max().unwrap_or(2).max(2);
    println!("{:<10} {:<name_w$} {:<id_w$} STATUS", "PLATFORM", "NAME", "ID", name_w = name_w, id_w = id_w);
    for d in &devices {
        println!("{:<10} {:<name_w$} {:<id_w$} {}", d.platform, d.name, d.id, d.status, name_w = name_w, id_w = id_w);
    }
}

/// Gathers every device this host can actually see right now. Never
/// errors — a platform whose tooling isn't installed just contributes no
/// entries (that's what `rsc doctor` is for diagnosing, not this command).
pub fn list_devices() -> Vec<Device> {
    let mut devices = vec![Device {
        platform: "macos",
        name: "This Mac".to_string(),
        id: "macos".to_string(),
        status: "desktop".to_string(),
    }];
    devices.extend(list_ios_simulators());
    devices.extend(list_android_devices());
    devices
}

/// Parses `xcrun simctl list devices available`'s plain-text output — one
/// device per line, `<name> (<udid>) (<state>)`. Deliberately not `-j`
/// (no JSON dependency in this crate).
///
/// The device NAME itself can contain parentheses — real examples on this
/// machine: "iPhone SE (3rd generation)", "iPad Pro (11-inch) (4th
/// generation)", "iPad mini (A17 Pro)". Naively splitting on the first `(`
/// breaks on every one of these (confirmed: it swallowed "3rd generation"
/// as the "UDID"). The UDID is always a UUID (`8-4-4-4-12` hex), which
/// nothing else on the line can be confused for, so it's found by shape
/// instead of position.
fn list_ios_simulators() -> Vec<Device> {
    let Ok(output) = Command::new("xcrun").args(["simctl", "list", "devices", "available"]).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in text.lines() {
        if !line.starts_with("    ") { continue } // skip section headers / blanks
        let Some((udid, udid_start)) = find_uuid(line) else { continue };
        // The name is everything before the "(<udid>)" group's opening paren.
        let name = line[..udid_start].trim_end().trim_end_matches('(').trim();
        if name.is_empty() { continue }
        let status = if line.contains("(Booted)") { "booted" } else { "shutdown" };
        devices.push(Device {
            platform: "ios",
            name: name.to_string(),
            id: udid.to_string(),
            status: status.to_string(),
        });
    }
    devices
}

/// Finds the first `8-4-4-4-12` hex-and-dashes UUID substring in `s`,
/// returning it plus its start byte offset. `simctl`'s device UDIDs are
/// always uppercase hex, but this matches case-insensitively since nothing
/// else on the line could coincidentally match the shape either way.
/// `pub(crate)` — reused by `run.rs`'s `resolve_simulator_udid` so the two
/// don't maintain separate (and, before this, separately-buggy) copies of
/// the same parsing logic.
pub(crate) fn find_uuid(s: &str) -> Option<(&str, usize)> {
    let bytes = s.as_bytes();
    let groups = [8, 4, 4, 4, 12];
    'outer: for start in 0..bytes.len() {
        let mut pos = start;
        for (i, &len) in groups.iter().enumerate() {
            if pos + len > bytes.len() { continue 'outer; }
            if !s[pos..pos + len].bytes().all(|b| b.is_ascii_hexdigit()) { continue 'outer; }
            pos += len;
            if i < groups.len() - 1 {
                if bytes.get(pos) != Some(&b'-') { continue 'outer; }
                pos += 1;
            }
        }
        return Some((&s[start..pos], start));
    }
    None
}

/// Parses `adb devices -l` — one connected device/emulator per line after
/// the header, `<serial>\tdevice <key:value...>`.
fn list_android_devices() -> Vec<Device> {
    let Ok(output) = Command::new("adb").args(["devices", "-l"]).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in text.lines().skip(1) {
        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else { continue };
        let Some(state) = parts.next() else { continue };
        if serial.is_empty() { continue }
        let model = parts
            .find_map(|p| p.strip_prefix("model:"))
            .unwrap_or(serial)
            .replace('_', " ");
        devices.push(Device {
            platform: "android",
            name: model,
            id: serial.to_string(),
            status: state.to_string(),
        });
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_uuid_matches_a_plain_uuid() {
        let (uuid, start) = find_uuid("DA884712-56EF-4605-A4FD-C00865FCC084").unwrap();
        assert_eq!(uuid, "DA884712-56EF-4605-A4FD-C00865FCC084");
        assert_eq!(start, 0);
    }

    #[test]
    fn find_uuid_ignores_parens_in_the_device_name_itself() {
        // Real device line shape: name contains its own "(...)" group before
        // the actual UDID's parens — naive first-paren parsing breaks here.
        let line = "    iPhone SE (3rd generation) (73639DDE-6A40-421E-AA0F-465B8E3349C5) (Shutdown) ";
        let (uuid, start) = find_uuid(line).unwrap();
        assert_eq!(uuid, "73639DDE-6A40-421E-AA0F-465B8E3349C5");
        let name = line[..start].trim_end().trim_end_matches('(').trim();
        assert_eq!(name, "iPhone SE (3rd generation)");
    }

    #[test]
    fn find_uuid_returns_none_when_absent() {
        assert!(find_uuid("no uuid on this line at all").is_none());
    }

    #[test]
    fn list_devices_always_includes_the_host_desktop_entry() {
        let devices = list_devices();
        assert!(devices.iter().any(|d| d.platform == "macos" && d.id == "macos"));
    }
}
