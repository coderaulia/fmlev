use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use crate::error::Result;
use tracing::debug;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryMapping {
    pub start: usize,
    pub end: usize,
    pub perms: String,
    pub pathname: String,
}

impl MemoryMapping {
    #[inline]
    pub fn size(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.perms.starts_with('r')
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.perms.chars().nth(1) == Some('w')
    }

    #[inline]
    pub fn is_executable(&self) -> bool {
        self.perms.chars().nth(2) == Some('x')
    }

    #[inline]
    pub fn is_private(&self) -> bool {
        self.perms.chars().nth(3) == Some('p')
    }

    #[inline]
    pub fn is_rw(&self) -> bool {
        self.is_readable() && self.is_writable()
    }

    #[inline]
    pub fn is_rw_private(&self) -> bool {
        self.is_rw() && self.is_private()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedModule {
    pub name: String,
    pub base_address: usize,
    pub size: usize,
    pub path: String,
}

#[derive(Debug, Clone, Default)]
pub struct ModuleBases {
    pub game_assembly_base: Option<usize>,
    pub unity_player_base: Option<usize>,
    pub fm_exe_base: Option<usize>,
}

impl ModuleBases {
    pub fn is_ready(&self) -> bool {
        self.game_assembly_base.is_some()
    }
}

pub fn parse_process_maps(pid: i32) -> Result<Vec<MemoryMapping>> {
    let maps_path = format!("/proc/{}/maps", pid);
    let file = File::open(&maps_path)?;
    let reader = BufReader::new(file);

    let mut mappings = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let addrs: Vec<&str> = parts[0].split('-').collect();
        if addrs.len() != 2 {
            continue;
        }

        let start = usize::from_str_radix(addrs[0], 16).unwrap_or(0);
        let end = usize::from_str_radix(addrs[1], 16).unwrap_or(0);
        let perms = parts[1].to_string();
        let pathname = if parts.len() >= 6 {
            parts[5..].join(" ")
        } else {
            String::new()
        };

        mappings.push(MemoryMapping {
            start,
            end,
            perms,
            pathname,
        });
    }

    Ok(mappings)
}

/// Lists all readable and writable private memory regions (`rw-p`), representing dynamic heap/anonymous state.
pub fn find_rw_mappings(pid: i32) -> Result<Vec<MemoryMapping>> {
    let mappings = parse_process_maps(pid)?;
    Ok(mappings.into_iter().filter(|m| m.is_rw_private()).collect())
}

/// Alias for `find_rw_mappings`.
pub fn get_rw_regions(pid: i32) -> Result<Vec<MemoryMapping>> {
    find_rw_mappings(pid)
}

/// Identifies all loaded PE modules (`.dll`, `.exe`), returning their base addresses, sizes, and paths.
pub fn find_loaded_modules(pid: i32) -> Result<Vec<LoadedModule>> {
    let mappings = parse_process_maps(pid)?;
    Ok(extract_loaded_modules(&mappings))
}

pub fn extract_loaded_modules(mappings: &[MemoryMapping]) -> Vec<LoadedModule> {
    // Group mappings by pathname
    // Key: pathname, Value: (min_start, max_end, path)
    let mut module_map: HashMap<String, (usize, usize, String)> = HashMap::new();

    for map in mappings {
        let p = map.pathname.trim();
        if p.is_empty() || p.starts_with('[') {
            continue;
        }

        let p_lower = p.to_lowercase();
        if p_lower.ends_with(".dll")
            || p_lower.ends_with(".exe")
            || p_lower.contains(".dll ")
            || p_lower.contains(".exe ")
        {
            let entry = module_map
                .entry(p.to_string())
                .or_insert((map.start, map.end, p.to_string()));
            if map.start < entry.0 {
                entry.0 = map.start;
            }
            if map.end > entry.1 {
                entry.1 = map.end;
            }
        }
    }

    let mut modules: Vec<LoadedModule> = module_map
        .into_iter()
        .map(|(_, (start, end, path))| {
            let name = path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(&path)
                .to_string();
            LoadedModule {
                name,
                base_address: start,
                size: end.saturating_sub(start),
                path,
            }
        })
        .collect();

    modules.sort_by_key(|m| m.base_address);
    modules
}

pub fn find_module_bases(pid: i32) -> Result<ModuleBases> {
    let mappings = parse_process_maps(pid)?;

    let mut game_assembly_base = None;
    let mut unity_player_base = None;
    let mut fm_exe_base = None;

    for map in mappings {
        let path_lower = map.pathname.to_lowercase();
        if path_lower.contains("gameassembly.dll") && game_assembly_base.is_none() {
            debug!("Found GameAssembly.dll at 0x{:x}", map.start);
            game_assembly_base = Some(map.start);
        } else if path_lower.contains("unityplayer.dll") && unity_player_base.is_none() {
            debug!("Found UnityPlayer.dll at 0x{:x}", map.start);
            unity_player_base = Some(map.start);
        } else if (path_lower.ends_with("/fm.exe") || path_lower.contains("fm.exe"))
            && fm_exe_base.is_none()
        {
            debug!("Found fm.exe at 0x{:x}", map.start);
            fm_exe_base = Some(map.start);
        }
    }

    Ok(ModuleBases {
        game_assembly_base,
        unity_player_base,
        fm_exe_base,
    })
}
