//! Santé disque réelle : NVMe SMART (log 0x02), ATA SMART via PredictFailure,
//! TRIM / défrag selon le média (jamais de défrag mécanique sur SSD).
use serde::Serialize;
use std::collections::HashMap;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    BusTypeNvme, BusTypeRAID, BusTypeSas, BusTypeSata, BusTypeScsi, BusTypeUsb, CreateFileW,
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW, GetVolumeInformationW,
    FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
    OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    DEVICE_SEEK_PENALTY_DESCRIPTOR, DEVICE_TRIM_DESCRIPTOR, IOCTL_STORAGE_PREDICT_FAILURE,
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PREDICT_FAILURE,
    STORAGE_PROPERTY_QUERY, STORAGE_PROTOCOL_DATA_DESCRIPTOR, STORAGE_PROTOCOL_SPECIFIC_DATA,
    STORAGE_PROTOCOL_TYPE, STORAGE_QUERY_TYPE, VOLUME_DISK_EXTENTS, NVMeDataTypeLogPage,
    PropertyStandardQuery, ProtocolTypeNvme, StorageDeviceProperty,
    StorageDeviceProtocolSpecificProperty, StorageDeviceSeekPenaltyProperty,
    StorageDeviceTrimProperty,
};
use windows::Win32::System::IO::DeviceIoControl;

const NO_WINDOW: u32 = 0x0800_0000;
const NVME_SMART_LID: u32 = 0x02;
const NVME_NSID_ALL: u32 = 0xFFFF_FFFF;

#[derive(Serialize, Clone)]
pub struct DiskVolume {
    pub letter: String,
    pub label: String,
    pub fs: String,
    pub total_gb: f64,
    pub free_gb: f64,
    pub free_pct: f32,
}

#[derive(Serialize, Clone)]
pub struct PhysicalDisk {
    pub index: u32,
    pub model: String,
    pub bus: String,
    pub kind: String,
    pub status: String,
    pub status_label: String,
    pub advice: Vec<String>,
    pub temp_c: Option<i32>,
    pub wear_pct: Option<u8>,
    pub spare_pct: Option<u8>,
    pub spare_threshold: Option<u8>,
    pub power_on_hours: Option<u64>,
    pub media_errors: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub reallocated: Option<u64>,
    pub pending: Option<u64>,
    pub uncorrectable: Option<u64>,
    pub predict_failure: bool,
    pub trim: bool,
    pub defrag_useful: bool,
    pub optimize_action: String,
    pub source: String,
    pub volumes: Vec<DiskVolume>,
    pub frag_pct: Option<u32>,
}

#[derive(Serialize, Clone)]
pub struct DiskJob {
    pub ok: bool,
    pub letter: String,
    pub action: String,
    pub log: String,
}

static CACHE: Mutex<Option<(Instant, Vec<PhysicalDisk>)>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn open_dev(path: &str) -> Option<HANDLE> {
    let w = wide(path);
    unsafe {
        let h = CreateFileW(
            PCWSTR(w.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        if h == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(h)
        }
    }
}

fn ioctl<T: Copy>(h: HANDLE, code: u32, out: &mut T) -> bool {
    let mut ret = 0u32;
    unsafe {
        DeviceIoControl(
            h,
            code,
            None,
            0,
            Some(out as *mut T as *mut _),
            size_of::<T>() as u32,
            Some(&mut ret),
            None,
        )
        .is_ok()
    }
}

fn ioctl_in_out(h: HANDLE, code: u32, input: &[u8], output: &mut [u8]) -> Option<u32> {
    let mut ret = 0u32;
    unsafe {
        DeviceIoControl(
            h,
            code,
            Some(input.as_ptr() as *const _),
            input.len() as u32,
            Some(output.as_mut_ptr() as *mut _),
            output.len() as u32,
            Some(&mut ret),
            None,
        )
        .ok()?;
    }
    Some(ret)
}

fn query_property(h: HANDLE, property: i32, out: &mut [u8]) -> bool {
    let mut q: STORAGE_PROPERTY_QUERY = unsafe { zeroed() };
    q.PropertyId = windows::Win32::System::Ioctl::STORAGE_PROPERTY_ID(property);
    q.QueryType = STORAGE_QUERY_TYPE(PropertyStandardQuery.0);
    let bytes = unsafe {
        std::slice::from_raw_parts(
            &q as *const _ as *const u8,
            size_of::<STORAGE_PROPERTY_QUERY>(),
        )
    };
    ioctl_in_out(h, IOCTL_STORAGE_QUERY_PROPERTY, bytes, out).is_some()
}

fn bus_name(b: i32) -> &'static str {
    if b == BusTypeNvme.0 {
        "NVMe"
    } else if b == BusTypeSata.0 {
        "SATA"
    } else if b == BusTypeUsb.0 {
        "USB"
    } else if b == BusTypeScsi.0 {
        "SCSI"
    } else if b == BusTypeSas.0 {
        "SAS"
    } else if b == BusTypeRAID.0 {
        "RAID"
    } else {
        "Autre"
    }
}

fn read_desc(h: HANDLE) -> (String, i32) {
    let mut buf = vec![0u8; 1024];
    if !query_property(h, StorageDeviceProperty.0, &mut buf) {
        return ("Disque".into(), 0);
    }
    if buf.len() < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
        return ("Disque".into(), 0);
    }
    let desc: STORAGE_DEVICE_DESCRIPTOR =
        unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };
    let mut model = String::new();
    for off in [desc.VendorIdOffset, desc.ProductIdOffset] {
        if off > 0 && (off as usize) < buf.len() {
            let s = cstr_at(&buf, off as usize);
            if !s.is_empty() {
                if !model.is_empty() {
                    model.push(' ');
                }
                model.push_str(&s);
            }
        }
    }
    if model.is_empty() {
        model = "Disque".into();
    }
    (model.trim().to_string(), desc.BusType.0)
}

fn cstr_at(buf: &[u8], off: usize) -> String {
    let slice = &buf[off..];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
    String::from_utf8_lossy(&slice[..end]).trim().to_string()
}

fn seek_penalty(h: HANDLE) -> Option<bool> {
    let mut buf = vec![0u8; size_of::<DEVICE_SEEK_PENALTY_DESCRIPTOR>() + 16];
    if !query_property(h, StorageDeviceSeekPenaltyProperty.0, &mut buf) {
        return None;
    }
    let d: DEVICE_SEEK_PENALTY_DESCRIPTOR =
        unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const _) };
    Some(d.IncursSeekPenalty.0 != 0)
}

fn trim_enabled(h: HANDLE) -> bool {
    let mut buf = vec![0u8; size_of::<DEVICE_TRIM_DESCRIPTOR>() + 16];
    if !query_property(h, StorageDeviceTrimProperty.0, &mut buf) {
        return false;
    }
    let d: DEVICE_TRIM_DESCRIPTOR = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const _) };
    d.TrimEnabled.0 != 0
}

fn predict_failure(h: HANDLE) -> (bool, [u8; 512]) {
    let mut p: STORAGE_PREDICT_FAILURE = unsafe { zeroed() };
    if ioctl(h, IOCTL_STORAGE_PREDICT_FAILURE, &mut p) {
        (p.PredictFailure != 0, p.VendorSpecific)
    } else {
        (false, [0; 512])
    }
}

/// NVMe SMART / Health Information Log (spec 1.4 §5.14.1.2, LID 02h).
#[derive(Default, Clone)]
struct NvmeSmart {
    critical: u8,
    temp_c: i32,
    spare: u8,
    spare_th: u8,
    used: u8,
    power_on_hours: u64,
    media_errors: u64,
    unsafe_shutdowns: u64,
}

fn u128_low_u64(b: &[u8]) -> u64 {
    if b.len() < 8 {
        return 0;
    }
    u64::from_le_bytes(b[..8].try_into().unwrap_or([0; 8]))
}

fn parse_nvme_health(log: &[u8]) -> Option<NvmeSmart> {
    if log.len() < 176 {
        return None;
    }
    let kelvin = u16::from_le_bytes([log[1], log[2]]) as i32;
    Some(NvmeSmart {
        critical: log[0],
        temp_c: kelvin - 273,
        spare: log[3],
        spare_th: log[4],
        used: log[5],
        power_on_hours: u128_low_u64(&log[128..144]),
        unsafe_shutdowns: u128_low_u64(&log[144..160]),
        media_errors: u128_low_u64(&log[160..176]),
    })
}

fn nvme_smart(h: HANDLE) -> Option<NvmeSmart> {
    // Query + protocol specific + 512-byte log (Microsoft: 512-byte chunks for NVMe log pages).
    #[repr(C)]
    struct Query {
        property_id: i32,
        query_type: i32,
        proto: STORAGE_PROTOCOL_SPECIFIC_DATA,
        data: [u8; 512],
    }
    let mut q: Query = unsafe { zeroed() };
    q.property_id = StorageDeviceProtocolSpecificProperty.0;
    q.query_type = PropertyStandardQuery.0;
    q.proto.ProtocolType = STORAGE_PROTOCOL_TYPE(ProtocolTypeNvme.0);
    q.proto.DataType = NVMeDataTypeLogPage.0 as u32;
    q.proto.ProtocolDataRequestValue = NVME_SMART_LID;
    q.proto.ProtocolDataRequestSubValue = NVME_NSID_ALL;
    q.proto.ProtocolDataOffset = size_of::<STORAGE_PROTOCOL_SPECIFIC_DATA>() as u32;
    q.proto.ProtocolDataLength = 512;

    let input = unsafe {
        std::slice::from_raw_parts(&q as *const _ as *const u8, size_of::<Query>())
    };
    let mut output = vec![0u8; size_of::<Query>() + 64];
    ioctl_in_out(h, IOCTL_STORAGE_QUERY_PROPERTY, input, &mut output)?;

    if output.len() < size_of::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>() {
        return None;
    }
    let desc: STORAGE_PROTOCOL_DATA_DESCRIPTOR =
        unsafe { std::ptr::read_unaligned(output.as_ptr() as *const _) };
    // Windows sometimes reports Size as sizeof(descriptor) rather than Version=1.
    if desc.Size == 0 && desc.Version == 0 {
        return None;
    }
    let proto = desc.ProtocolSpecificData;
    let start = size_of::<u32>() * 2 + proto.ProtocolDataOffset as usize;
    let len = proto.ProtocolDataLength as usize;
    if start + 176 > output.len() || len < 176 {
        // Fallback: log placed right after protocol struct in our query layout.
        let alt = size_of::<STORAGE_PROTOCOL_DATA_DESCRIPTOR>();
        if alt + 176 <= output.len() {
            return parse_nvme_health(&output[alt..]);
        }
        return None;
    }
    parse_nvme_health(&output[start..start + len.min(512)])
}

#[derive(Default, Clone)]
struct AtaSmart {
    reallocated: u64,
    pending: u64,
    uncorrectable: u64,
    temp_c: Option<i32>,
    power_on_hours: Option<u64>,
}

fn ata_attr_raw(rec: &[u8]) -> u64 {
    let mut b = [0u8; 8];
    b[..6].copy_from_slice(&rec[5..11]);
    u64::from_le_bytes(b)
}

fn parse_ata_smart(data: &[u8; 512]) -> Option<AtaSmart> {
    // SMART READ DATA : révision + 30 attributs de 12 octets.
    let rev = u16::from_le_bytes([data[0], data[1]]);
    if rev == 0 || rev == 0xFFFF {
        return None;
    }
    let mut a = AtaSmart::default();
    let mut any = false;
    for i in 0..30 {
        let o = 2 + i * 12;
        if o + 12 > data.len() {
            break;
        }
        let id = data[o];
        if id == 0 {
            continue;
        }
        any = true;
        let raw = ata_attr_raw(&data[o..o + 12]);
        match id {
            5 => a.reallocated = raw,
            9 => a.power_on_hours = Some(raw),
            187 | 198 => {
                if raw > a.uncorrectable {
                    a.uncorrectable = raw;
                }
            }
            197 => a.pending = raw,
            190 | 194 => a.temp_c = Some((raw & 0xFF) as i32),
            _ => {}
        }
    }
    any.then_some(a)
}

fn kind_of(bus: i32, penalty: Option<bool>, trim: bool, nvme: bool) -> String {
    if nvme || bus == BusTypeNvme.0 {
        "NVMe SSD".into()
    } else if penalty == Some(false) || (trim && penalty != Some(true)) {
        "SSD".into()
    } else if penalty == Some(true) {
        "HDD".into()
    } else {
        "Disque".into()
    }
}

fn is_hdd(kind: &str) -> bool {
    kind == "HDD"
}

fn is_flash(kind: &str) -> bool {
    kind.contains("SSD") || kind.contains("NVMe")
}

/// Verdict NVMe spec + SMART ATA + espace libre. Conservateur : le doute → « à surveiller ».
fn verdict(d: &mut PhysicalDisk) {
    let mut score = 0u8; // 0 ok, 1 watch, 2 bad, 3 critical
    let mut advice = Vec::new();

    if d.predict_failure {
        score = 3;
        advice.push("Windows PredictFailure : le firmware signale une panne imminente. Sauvegarde tout de suite, remplace le disque.".into());
    }
    if let Some(nv) = d.wear_pct {
        if nv >= 100 {
            score = score.max(3);
            advice.push(format!("Usure NVMe {nv} % : l’endurance constructeur est consommée (Percentage Used, log SMART 02h)."));
        } else if nv >= 90 {
            score = score.max(2);
            advice.push(format!("Usure NVMe {nv} % : le disque arrive en fin de vie. Prépare un remplacement."));
        } else if nv >= 70 {
            score = score.max(1);
            advice.push(format!("Usure NVMe {nv} % : encore bon, mais à surveiller (seuil 70 / 90 / 100)."));
        }
    }
    if let (Some(sp), Some(th)) = (d.spare_pct, d.spare_threshold) {
        if sp < th {
            score = score.max(3);
            advice.push(format!("Spare NVMe {sp} % sous le seuil {th} % (Critical Warning bit 0). Disque à remplacer."));
        } else if sp < 20 {
            score = score.max(2);
            advice.push(format!("Spare NVMe bas ({sp} %). Les blocs de réserve s’épuisent."));
        }
    }
    if d.media_errors.unwrap_or(0) > 0 {
        score = score.max(3);
        advice.push(format!(
            "{} erreurs média NVMe non corrigées. Sauvegarde, puis clone vers un disque neuf.",
            d.media_errors.unwrap_or(0)
        ));
    }
    if d.reallocated.unwrap_or(0) > 200 || d.uncorrectable.unwrap_or(0) > 0 {
        score = score.max(3);
        advice.push("SMART ATA : secteurs réalloués/uncorrectable trop élevés. Le plateau lâche.".into());
    } else if d.pending.unwrap_or(0) > 0 || d.reallocated.unwrap_or(0) > 0 {
        score = score.max(1);
        advice.push("SMART ATA : secteurs en attente ou réalloués. Surveille ; si ça monte, remplace.".into());
    }
    if let Some(t) = d.temp_c {
        if t >= 80 {
            score = score.max(2);
            advice.push(format!("{t} °C : trop chaud (seuil spec souvent 70–80 °C). Airflow / dépoussiérage."));
        } else if t >= 70 {
            score = score.max(1);
            advice.push(format!("{t} °C : limite haute. Laisse de la place autour du SSD / HDD."));
        }
    }

    let min_free = d
        .volumes
        .iter()
        .map(|v| v.free_pct)
        .fold(100f32, f32::min);
    if is_flash(&d.kind) && min_free < 10.0 {
        score = score.max(1);
        advice.push("Moins de 10 % d’espace libre : le wear-leveling SSD souffre. Libère de la place.".into());
    } else if min_free < 5.0 {
        score = score.max(2);
        advice.push("Volume quasi plein (< 5 %). Windows et le disque rament. Nettoie.".into());
    }

    if is_hdd(&d.kind) {
        d.defrag_useful = true;
        d.optimize_action = "Défragmenter".into();
        if let Some(f) = d.frag_pct {
            if f >= 10 {
                score = score.max(1);
                advice.push(format!("Fragmentation {f} % : la défrag Windows est utile sur un HDD (pas sur SSD)."));
            } else {
                advice.push(format!("Fragmentation {f} % : pas urgent. Optimiser reste sans danger."));
            }
        } else {
            advice.push("Disque mécanique : la défrag aide les temps de seek. « Optimiser » lance defrag /O.".into());
        }
    } else if is_flash(&d.kind) {
        d.defrag_useful = false;
        d.optimize_action = "TRIM / optimiser".into();
        advice.push("SSD/NVMe : ne pas défragmenter (ça use les cellules). Windows doit faire un TRIM (defrag /O ou /L).".into());
        if !d.trim {
            score = score.max(1);
            advice.push("TRIM non annoncé par le pilote. « Améliorer » réactive DeleteNotify puis retrim.".into());
        }
    } else {
        d.optimize_action = "Optimiser".into();
    }

    if d.unsafe_shutdowns.unwrap_or(0) > 200 {
        advice.push("Beaucoup d’extinctions brutales : évite de couper l’alim, ça fatigue le mapping NVMe.".into());
    }

    if advice.is_empty() {
        advice.push("SMART OK : pas de PredictFailure, spare/usure dans les clous, température saine.".into());
    }

    let (status, label) = match score {
        0 => ("ok", "Bon"),
        1 => ("watch", "À surveiller"),
        2 => ("bad", "Usé"),
        _ => ("critical", "Critique"),
    };
    d.status = status.into();
    d.status_label = label.into();
    d.advice = advice;
}

fn volumes_by_disk() -> HashMap<u32, Vec<DiskVolume>> {
    let mut map: HashMap<u32, Vec<DiskVolume>> = HashMap::new();
    let mut names = vec![0u16; 256];
    let n = unsafe { GetLogicalDriveStringsW(Some(&mut names)) } as usize;
    if n == 0 {
        return map;
    }
    let s = String::from_utf16_lossy(&names[..n]);
    for part in s.split('\0').filter(|p| p.len() >= 2) {
        let root = if part.ends_with('\\') {
            part.to_string()
        } else {
            format!("{part}\\")
        };
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide(&root).as_ptr())) };
        // DRIVE_FIXED = 3
        if drive_type != 3 {
            continue;
        }
        let letter = root.chars().next().unwrap_or('?').to_string() + ":";
        let mut total = 0u64;
        let mut free = 0u64;
        unsafe {
            let _ = GetDiskFreeSpaceExW(
                PCWSTR(wide(&root).as_ptr()),
                None,
                Some(&mut total),
                Some(&mut free),
            );
        }
        let mut label_buf = [0u16; 64];
        let mut fs_buf = [0u16; 16];
        let mut serial = 0u32;
        let mut max_comp = 0u32;
        let mut flags = 0u32;
        let _ = unsafe {
            GetVolumeInformationW(
                PCWSTR(wide(&root).as_ptr()),
                Some(&mut label_buf),
                Some(&mut serial),
                Some(&mut max_comp),
                Some(&mut flags),
                Some(&mut fs_buf),
            )
        };
        let label = String::from_utf16_lossy(&label_buf)
            .trim_matches('\0')
            .trim()
            .to_string();
        let fs = String::from_utf16_lossy(&fs_buf)
            .trim_matches('\0')
            .trim()
            .to_string();
        let total_gb = total as f64 / 1_073_741_824.0;
        let free_gb = free as f64 / 1_073_741_824.0;
        let free_pct = if total > 0 {
            (free as f64 / total as f64) * 100.0
        } else {
            0.0
        } as f32;

        let idx = volume_disk_index(&letter).unwrap_or(0);
        map.entry(idx).or_default().push(DiskVolume {
            letter,
            label,
            fs,
            total_gb: (total_gb * 10.0).round() / 10.0,
            free_gb: (free_gb * 10.0).round() / 10.0,
            free_pct: (free_pct * 10.0).round() / 10.0,
        });
    }
    map
}

fn volume_disk_index(letter: &str) -> Option<u32> {
    let path = format!(r"\\.\{}", letter.trim_end_matches('\\'));
    let h = open_dev(&path)?;
    let mut ext: VOLUME_DISK_EXTENTS = unsafe { zeroed() };
    let ok = ioctl(h, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, &mut ext);
    unsafe {
        let _ = CloseHandle(h);
    }
    if ok && ext.NumberOfDiskExtents >= 1 {
        Some(ext.Extents[0].DiskNumber)
    } else {
        None
    }
}

pub fn list_disks() -> Vec<PhysicalDisk> {
    let now = Instant::now();
    if let Ok(g) = CACHE.lock() {
        if let Some((at, cached)) = g.as_ref() {
            if now.duration_since(*at) < Duration::from_secs(20) {
                return cached.clone();
            }
        }
    }
    let vols = volumes_by_disk();
    let mut disks = Vec::new();
    for index in 0..16u32 {
        let path = format!(r"\\.\PhysicalDrive{index}");
        let Some(h) = open_dev(&path) else {
            continue;
        };
        let (model, bus) = read_desc(h);
        let penalty = seek_penalty(h);
        let trim = trim_enabled(h);
        let (fail, vendor) = predict_failure(h);
        let nv = nvme_smart(h);
        let ata = if nv.is_none() {
            parse_ata_smart(&vendor)
        } else {
            None
        };
        unsafe {
            let _ = CloseHandle(h);
        }

        let kind = kind_of(bus, penalty, trim, nv.is_some());
        let source = if nv.is_some() {
            "NVMe SMART log 02h"
        } else if ata.is_some() {
            "ATA SMART (PredictFailure)"
        } else {
            "Windows Storage"
        };

        let mut d = PhysicalDisk {
            index,
            model,
            bus: bus_name(bus).into(),
            kind,
            status: "ok".into(),
            status_label: "Bon".into(),
            advice: Vec::new(),
            temp_c: nv.as_ref().map(|n| n.temp_c).or(ata.as_ref().and_then(|a| a.temp_c)),
            wear_pct: nv.as_ref().map(|n| n.used),
            spare_pct: nv.as_ref().map(|n| n.spare),
            spare_threshold: nv.as_ref().map(|n| n.spare_th),
            power_on_hours: nv
                .as_ref()
                .map(|n| n.power_on_hours)
                .or(ata.as_ref().and_then(|a| a.power_on_hours)),
            media_errors: nv.as_ref().map(|n| n.media_errors),
            unsafe_shutdowns: nv.as_ref().map(|n| n.unsafe_shutdowns),
            reallocated: ata.as_ref().map(|a| a.reallocated),
            pending: ata.as_ref().map(|a| a.pending),
            uncorrectable: ata.as_ref().map(|a| a.uncorrectable),
            predict_failure: fail || nv.as_ref().map(|n| n.critical != 0).unwrap_or(false),
            trim,
            defrag_useful: false,
            optimize_action: "Optimiser".into(),
            source: source.into(),
            volumes: vols.get(&index).cloned().unwrap_or_default(),
            frag_pct: None,
        };
        if let Some(n) = &nv {
            if n.critical != 0 {
                d.predict_failure = true;
            }
        }
        verdict(&mut d);
        disks.push(d);
    }
    if let Ok(mut g) = CACHE.lock() {
        *g = Some((now, disks.clone()));
    }
    disks
}

fn invalidate() {
    if let Ok(mut g) = CACHE.lock() {
        *g = None;
    }
}

fn run_hidden(cmd: &str, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new(cmd);
    c.args(args);
    c.creation_flags(NO_WINDOW);
    let out = c
        .output()
        .map_err(|e| format!("Impossible de lancer {cmd} : {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let text = format!("{stdout}\n{stderr}").trim().to_string();
    if out.status.code() == Some(5)
        || (!out.status.success() && text.to_lowercase().contains("refus"))
    {
        return Err(
            "Accès refusé. Relance Mode Jeu en administrateur pour optimiser le volume système."
                .into(),
        );
    }
    if text.is_empty() && !out.status.success() {
        return Err(format!(
            "{cmd} a échoué (code {}). Lance l’app en administrateur si c’est C:.",
            out.status.code().unwrap_or(-1)
        ));
    }
    Ok(text)
}

fn letter_ok(letter: &str) -> Result<String, String> {
    let ch = letter
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| "Lettre de volume invalide.".to_string())?;
    Ok(format!("{}:", ch.to_ascii_uppercase()))
}

/// `defrag /O` : TRIM sur SSD, défrag sur HDD — c’est le bon outil Windows.
pub fn optimize_volume(letter: &str) -> Result<DiskJob, String> {
    let l = letter_ok(letter)?;
    let log = run_hidden("defrag", &[&l, "/O"])?;
    invalidate();
    Ok(DiskJob {
        ok: true,
        letter: l,
        action: "optimize".into(),
        log,
    })
}

/// Analyse fragmentation (`defrag /A`). Utile uniquement sur HDD.
pub fn analyze_fragmentation(letter: &str) -> Result<DiskJob, String> {
    let l = letter_ok(letter)?;
    let log = run_hidden("defrag", &[&l, "/A"])?;
    let frag = parse_frag_pct(&log);
    if let Ok(mut g) = CACHE.lock() {
        if let Some((_, disks)) = g.as_mut() {
            for d in disks.iter_mut() {
                if d.volumes.iter().any(|v| v.letter.eq_ignore_ascii_case(&l)) {
                    d.frag_pct = frag;
                    verdict(d);
                }
            }
        }
    }
    Ok(DiskJob {
        ok: true,
        letter: l,
        action: "frag".into(),
        log,
    })
}

fn parse_frag_pct(log: &str) -> Option<u32> {
    let low = log.to_lowercase();
    // FR : « fragmentation ... 12 % »  EN : « fragmentation ... 12% »
    let idx = low.find("fragment")?;
    let rest = &low[idx..];
    for token in rest.split(|c: char| !c.is_ascii_digit()) {
        if let Ok(n) = token.parse::<u32>() {
            if n <= 100 {
                return Some(n);
            }
        }
    }
    None
}

/// Script d’entretien : TRIM Windows + optimisation adaptée au média.
pub fn improve_volume(letter: &str) -> Result<DiskJob, String> {
    let l = letter_ok(letter)?;
    let mut parts = Vec::new();

    match run_hidden("fsutil", &["behavior", "query", "DisableDeleteNotify"]) {
        Ok(q) => {
            parts.push(q.clone());
            let off = q.to_lowercase().contains("disabledeletenotify = 1")
                || q.to_lowercase().contains("is 1");
            if off {
                match run_hidden("fsutil", &["behavior", "set", "DisableDeleteNotify", "0"]) {
                    Ok(s) => parts.push(s),
                    Err(e) => parts.push(e),
                }
            } else {
                parts.push("TRIM (DeleteNotify) déjà actif.".into());
            }
        }
        Err(e) => parts.push(e),
    }

    match run_hidden("defrag", &[&l, "/O"]) {
        Ok(s) => parts.push(s),
        Err(e) => parts.push(e),
    }

    invalidate();
    Ok(DiskJob {
        ok: true,
        letter: l,
        action: "improve".into(),
        log: parts.join("\n\n"),
    })
}
