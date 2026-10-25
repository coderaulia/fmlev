//! Read-only benchmark counterpart of fmscan player-data. No UI or write calibration.
//! cargo run --release -p fmlev-core --example player_data -- PID INDEX UID [--record HEX] [--iterations N]
use fmlev_core::native_player::native_attributes;
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver, MemoryMapping};
use serde_json::{json, Value};
use std::{collections::BTreeMap, error::Error, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CHUNK: usize = 1 << 20;
const POS: [&str; 15] = [
    "gk", "sw", "dl", "dc", "dr", "dm", "ml", "mc", "mr", "aml", "amc", "amr", "st", "wbl", "wbr",
];
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], o: usize) -> usize {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as usize
}
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn heap(r: &MemoryMapping) -> bool {
    r.perms == "rw-p" && (r.pathname.is_empty() || r.pathname == "[heap]")
}

struct Reader {
    driver: LinuxMemoryDriver,
    maps: Vec<MemoryMapping>,
}
impl Reader {
    fn region(&self, a: usize, n: usize) -> Option<&MemoryMapping> {
        let i = self.maps.partition_point(|r| r.start <= a);
        let r = i.checked_sub(1).map(|i| &self.maps[i])?;
        (a < r.end && n <= r.end - a).then_some(r)
    }
    fn bytes(&self, a: usize, b: &mut [u8]) -> Result<()> {
        if a.checked_add(b.len()).is_none() || self.driver.read_bytes(a, b)? != b.len() {
            return Err("short/overflowing read".into());
        }
        Ok(())
    }
    fn vtable(&self, vt: usize) -> bool {
        let Some(r) = self.region(vt, 8) else {
            return false;
        };
        if !r.is_readable() || r.is_writable() {
            return false;
        }
        self.driver
            .read_ptr(vt)
            .ok()
            .and_then(|p| self.region(p, 1))
            .is_some_and(|r| r.is_executable())
    }
    fn name(&self, reference: usize) -> Result<String> {
        if reference == 0 {
            return Ok(String::new());
        }
        let p = self.driver.read_ptr(reference)?;
        let n = self.driver.read::<u32>(p)? as usize;
        if n == 0 || n > 512 {
            return Err("invalid name length".into());
        }
        let mut bytes = vec![0; n];
        self.bytes(p.checked_add(4).ok_or("name overflow")?, &mut bytes)?;
        let s = String::from_utf8(bytes)?;
        if s.chars().any(char::is_control) {
            return Err("control in name".into());
        }
        Ok(s)
    }
    fn snapshot(&self, record: usize, index: u32, uid: u32) -> Result<Snapshot> {
        let mut b = [0; 0x200];
        self.bytes(record, &mut b)?;
        if !self.region(record, b.len()).is_some_and(heap)
            || u32_at(&b, 0x148) != index
            || u32_at(&b, 0x14c) != uid
            || u32_at(&b, 0x150) != uid
            || !self.vtable(u64_at(&b, 0))
            || !self.vtable(u64_at(&b, 0x140))
        {
            return Err("native snapshot identity/type mismatch".into());
        }
        let attributes = native_attributes()
            .map(|f| Ok(f.decode_snapshot(&b)?))
            .collect::<Result<Vec<_>>>()?;
        let mut positions = [0u8; 17];
        positions[..15].copy_from_slice(&b[8..23]);
        if positions[..15].iter().any(|v| !(1..=20).contains(v)) {
            return Err("invalid position".into());
        }
        for i in 0..2 {
            let v = b[0x2f + i];
            if v == 0 || v > 100 || v % 5 != 0 {
                return Err("invalid foot".into());
            }
            positions[15 + i] = v / 5;
        }
        let ca = u16_at(&b, 0x11c);
        let pa = u16_at(&b, 0x11e);
        let name = [self.name(u64_at(&b, 0x190))?, self.name(u64_at(&b, 0x198))?]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if name.is_empty() {
            return Err("missing name".into());
        }
        let contract = u64_at(&b, 0x1e8);
        let (mut wage, mut expiry, mut number) = (None, None, None);
        if contract != 0 {
            let mut c = [0; 0x60];
            self.bytes(contract, &mut c)?;
            if !self.vtable(u64_at(&c, 0)) || u64_at(&c, 8) != record + 0x140 {
                return Err("invalid contract ownership/type".into());
            }
            let packed = u32_at(&c, 0x48);
            let year = packed >> 16;
            let day = packed & 0x1ff;
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            if !(2000..=2200).contains(&year) || day == 0 || day > if leap { 366 } else { 365 } {
                return Err("invalid contract date".into());
            }
            wage = Some(u32_at(&c, 0x20));
            expiry = Some(packed);
            number = (c[0x5d] != 255).then_some(c[0x5d]);
        }
        let check = self.driver.read::<[u32; 3]>(record + 0x148)?;
        if check != [index, uid, uid] || self.driver.read_ptr(record + 0x1e8)? != contract {
            return Err("player changed during snapshot".into());
        }
        Ok(Snapshot {
            index,
            uid,
            name,
            attributes,
            positions,
            ca: if ca <= 200 { ca } else { 0 },
            pa: if pa <= 200 { pa } else { 0 },
            wage,
            expiry,
            number,
        })
    }
    fn locate(
        &self,
        index: u32,
        uid: u32,
        near: Option<(usize, usize)>,
    ) -> Result<(usize, u64, u64)> {
        let mut found = Vec::new();
        let mut buf = vec![0; CHUNK];
        let (mut bytes, mut unreadable) = (0, 0);
        let (lo, hi) = near
            .map(|(c, r)| (c.saturating_sub(r), c.saturating_add(r)))
            .unwrap_or((0, usize::MAX));
        for r in self
            .maps
            .iter()
            .filter(|r| heap(r) && r.end > lo && r.start <= hi)
        {
            let start = r.start.max(lo);
            let mut a = start.checked_add(7).ok_or("scan overflow")? & !7;
            let end = r.end.min(hi.saturating_add(1));
            while a < end {
                let n = CHUNK.min(end - a);
                if self.bytes(a, &mut buf[..n]).is_ok() {
                    bytes += n as u64;
                    for i in (0..n.saturating_sub(7)).step_by(8) {
                        if u64_at(&buf, i) as u64 != ((uid as u64) << 32 | index as u64) {
                            continue;
                        }
                        let duplicate = if i + 12 <= n {
                            u32_at(&buf, i + 8)
                        } else {
                            match self.driver.read::<u32>(a + i + 8) {
                                Ok(v) => v,
                                Err(_) => continue,
                            }
                        };
                        if duplicate != uid || a + i < 0x148 {
                            continue;
                        }
                        let candidate = a + i - 0x148;
                        if self.snapshot(candidate, index, uid).is_ok() {
                            found.push(candidate);
                        }
                    }
                } else {
                    unreadable += 1;
                }
                a += n;
            }
        }
        if bytes == 0 || found.len() != 1 {
            return Err(format!(
                "expected one readable native player snapshot candidate, found {}",
                found.len()
            )
            .into());
        }
        Ok((found[0], bytes, unreadable))
    }
}
struct Snapshot {
    index: u32,
    uid: u32,
    name: String,
    attributes: Vec<u8>,
    positions: [u8; 17],
    ca: u16,
    pa: u16,
    wage: Option<u32>,
    expiry: Option<u32>,
    number: Option<u8>,
}
impl Snapshot {
    fn checksum(&self) -> u64 {
        self.index as u64
            + self.uid as u64
            + self.ca as u64
            + self.pa as u64
            + self.wage.unwrap_or(0) as u64
            + self.expiry.unwrap_or(0) as u64
            + self.number.unwrap_or(0) as u64
            + self
                .attributes
                .iter()
                .chain(self.positions.iter())
                .map(|v| *v as u64)
                .sum::<u64>()
            + self.name.bytes().map(|v| v as u64).sum::<u64>()
    }
    fn json(&self) -> Value {
        let attrs: BTreeMap<_, _> = native_attributes()
            .zip(&self.attributes)
            .map(|(s, v)| (s.name, *v))
            .collect();
        let mut positions: BTreeMap<_, _> = POS
            .iter()
            .zip(&self.positions)
            .map(|(k, v)| (*k, *v))
            .collect();
        positions.insert("left_foot", self.positions[15]);
        positions.insert("right_foot", self.positions[16]);
        json!({"id": self.index, "unique_id": self.uid, "name": self.name,
            "current_ability": (self.ca > 0).then_some(self.ca), "potential_ability": (self.pa > 0).then_some(self.pa),
            "attributes": attrs, "positions": positions,
            "contract": {"wage_per_week": self.wage, "currency": "GBP", "expiry_year": self.expiry.map(|v| v >> 16),
                "expiry_packed": self.expiry.map(|v| format!("0x{v:x}")), "squad_number": self.number,
                "release_clause": null, "squad_status": null, "happiness": null}})
    }
}
fn numeric(s: &str, hex: bool) -> Result<usize> {
    Ok(if let Some(s) = s.strip_prefix("0x") {
        usize::from_str_radix(s, 16)?
    } else if hex {
        usize::from_str_radix(s, 16)?
    } else {
        s.parse()?
    })
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).filter(|s| s != "--json").collect();
    if args.len() < 3 {
        return Err(
            "usage: player_data PID INDEX UID [--record HEX] [--iterations N] [--near HEX:RADIUS]"
                .into(),
        );
    }
    let pid: i32 = args[0].parse()?;
    let index: u32 = args[1].parse()?;
    let uid: u32 = args[2].parse()?;
    if pid <= 0 || index == 0 || index > i32::MAX as u32 || uid == 0 {
        return Err("invalid PID/identity".into());
    }
    let mut record = None;
    let mut iterations = 1usize;
    let mut near = None;
    let mut options = std::collections::HashSet::new();
    for pair in args[3..].chunks(2) {
        if pair.len() != 2 || !options.insert(&pair[0]) {
            return Err("missing/duplicate option".into());
        }
        match pair[0].as_str() {
            "--record" => record = Some(numeric(&pair[1], true)?),
            "--iterations" => iterations = numeric(&pair[1], false)?,
            "--near" => {
                let (c, r) = pair[1].split_once(':').unwrap_or((&pair[1], "100"));
                near = Some((numeric(c, true)?, numeric(r, true)?));
            }
            _ => return Err("unknown option".into()),
        }
    }
    if !(1..=1_000_000).contains(&iterations) {
        return Err("invalid iterations".into());
    }
    let reader = Reader {
        driver: LinuxMemoryDriver::new(pid),
        maps: parse_process_maps(pid)?,
    };
    let mut scan_ns = 0;
    let (mut bytes, mut unreadable) = (0, 0);
    let record = if let Some(record) = record {
        record
    } else {
        let start = Instant::now();
        let (record, b, u) = reader.locate(index, uid, near)?;
        scan_ns = start.elapsed().as_nanos();
        bytes = b;
        unreadable = u;
        record
    };
    let mut checksum = 0u64;
    let mut snapshot = None;
    let start = Instant::now();
    for _ in 0..iterations {
        let p = reader.snapshot(record, index, uid)?;
        checksum = checksum.wrapping_add(p.checksum());
        snapshot = Some(p);
    }
    let read_ns = start.elapsed().as_nanos();
    println!(
        "{}",
        json!({"command": "player-data", "record": format!("0x{record:x}"), "player": snapshot.unwrap().json(),
        "scan_ns": scan_ns, "read_ns": read_ns, "iterations": iterations, "checksum": checksum, "scanned_bytes": bytes, "unreadable_chunks": unreadable})
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{}", json!({"error": e.to_string()}));
        std::process::exit(1);
    }
}
