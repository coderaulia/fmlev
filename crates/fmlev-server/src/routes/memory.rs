use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use tracing::info;

use fmlev_core::SquadActions;
use fmlev_mem::{
    find_fm_process, find_module_bases, get_yama_diagnostic, parse_process_maps,
    LinuxMemoryDriver,
};

use crate::state::{
    AttachRequest, FinanceRequest, GodModeRequest, HealRequest, MemReadRequest, MemScanRequest,
    MemWriteRequest, ScanMatch, ServerState,
};
use crate::DEFAULT_OFFSETS_JSON;
use super::api_error;

pub async fn attach_process(
    State(state): State<ServerState>,
    payload: Option<Json<AttachRequest>>,
) -> impl IntoResponse {
    let target_pid = payload.and_then(|p| p.pid);

    let proc_result = if let Some(pid) = target_pid {
        Ok(fmlev_mem::GameProcess {
            pid,
            exe_path: std::path::PathBuf::from(format!("/proc/{}/exe", pid)),
            is_proton: true,
            wine_prefix: None,
        })
    } else {
        find_fm_process()
    };

    match proc_result {
        Ok(proc) => {
            let pid = proc.pid;
            if pid <= 0 {
                return api_error(StatusCode::BAD_REQUEST, "PID must be positive");
            }
            if let Err(error) = parse_process_maps(pid) {
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            let bases = find_module_bases(pid).unwrap_or(fmlev_mem::ModuleBases {
                game_assembly_base: None,
                unity_player_base: None,
                fm_exe_base: None,
            });

            let mut w = state.inner.write().await;
            w.pid = Some(pid);
            w.exe_path = Some(proc.exe_path.to_string_lossy().to_string());
            w.is_proton = proc.is_proton;
            w.wine_prefix = proc.wine_prefix.map(|p| p.to_string_lossy().to_string());
            w.game_assembly_base = bases.game_assembly_base;
            w.unity_player_base = bases.unity_player_base;
            w.fm_exe_base = bases.fm_exe_base;
            if let Err(error) = w.context_mgr.attach_pid(pid) {
                w.is_attached = false;
                w.pid = None;
                return api_error(StatusCode::BAD_REQUEST, error);
            }
            w.is_attached = true;

            info!("Attached to Football Manager 26 PID: {}", pid);

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "attached",
                    "pid": pid,
                    "game_assembly_base": bases.game_assembly_base.map(|b| format!("0x{:x}", b)),
                    "unity_player_base": bases.unity_player_base.map(|b| format!("0x{:x}", b)),
                    "fm_exe_base": bases.fm_exe_base.map(|b| format!("0x{:x}", b)),
                })),
            )
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "status": "error",
                "message": e.to_string(),
                "yama": get_yama_diagnostic()
            })),
        ),
    }
}

pub async fn get_maps(State(state): State<ServerState>) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "status": "not_attached",
                    "message": "Not attached to any process. Run auto-attach first."
                })),
            )
        }
    };

    match parse_process_maps(pid) {
        Ok(mappings) => {
            let mut modules = Vec::new();
            let mut seen_paths = std::collections::HashSet::new();

            for map in &mappings {
                let path = map.pathname.trim();
                if path.is_empty() || path.starts_with('[') {
                    continue;
                }

                let path_lower = path.to_lowercase();
                if (path_lower.contains("gameassembly")
                    || path_lower.contains("unityplayer")
                    || path_lower.contains("fm.exe")
                    || path_lower.ends_with(".dll")
                    || path_lower.ends_with(".so"))
                    && seen_paths.insert(path.to_string())
                {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string());

                    modules.push(serde_json::json!({
                        "name": name,
                        "path": path,
                        "start": format!("0x{:x}", map.start),
                        "end": format!("0x{:x}", map.end),
                        "perms": map.perms,
                    }));
                }
            }

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "pid": pid,
                    "total_mappings": mappings.len(),
                    "modules": modules
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "status": "error",
                "message": e.to_string()
            })),
        ),
    }
}

pub fn parse_hex_or_dec_address(addr_str: &str) -> Option<usize> {
    let clean = addr_str.trim().trim_matches('"');
    if let Some(stripped) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        usize::from_str_radix(stripped, 16).ok()
    } else if clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        usize::from_str_radix(clean, 16).ok()
    } else {
        clean.parse::<usize>().ok()
    }
}

pub async fn read_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemReadRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "not_attached", "message": "Game not attached" })),
            )
        }
    };

    let target_addr = match parse_hex_or_dec_address(&payload.address) {
        Some(a) => a,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "invalid_address", "message": "Could not parse address" })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);

    match payload.r#type.as_str() {
        "u8" => match driver.read::<u8>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u8",
                    "value": v,
                    "hex": format!("0x{:02x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u16" => match driver.read::<u16>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u16",
                    "value": v,
                    "hex": format!("0x{:04x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "i32" => match driver.read::<i32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "i32",
                    "value": v,
                    "hex": format!("0x{:08x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u32" => match driver.read::<u32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u32",
                    "value": v,
                    "hex": format!("0x{:08x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "i64" => match driver.read::<i64>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "i64",
                    "value": v,
                    "hex": format!("0x{:016x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "u64" => match driver.read::<u64>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "u64",
                    "value": v,
                    "hex": format!("0x{:016x}", v)
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "f32" => match driver.read::<f32>(target_addr) {
            Ok(v) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "address": format!("0x{:x}", target_addr),
                    "type": "f32",
                    "value": v
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        },
        "string" => {
            let size = payload.size.clamp(1, 1024);
            let mut buf = vec![0u8; size];
            match driver.read_bytes(target_addr, &mut buf) {
                Ok(bytes_read) => {
                    let slice = &buf[..bytes_read];
                    let null_end = slice.iter().position(|&b| b == 0).unwrap_or(slice.len());
                    let s = String::from_utf8_lossy(&slice[..null_end]).to_string();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "address": format!("0x{:x}", target_addr),
                            "type": "string",
                            "value": s,
                            "length": null_end
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
        _ => {
            let size = payload.size.clamp(1, 4096);
            let mut buf = vec![0u8; size];
            match driver.read_bytes(target_addr, &mut buf) {
                Ok(bytes_read) => {
                    buf.truncate(bytes_read);
                    let hex_str = buf
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let ascii_str = buf
                        .iter()
                        .map(|&b| {
                            if (32..=126).contains(&b) {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect::<String>();

                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "address": format!("0x{:x}", target_addr),
                            "type": "bytes",
                            "bytes": buf,
                            "hex": hex_str,
                            "ascii": ascii_str,
                            "size": bytes_read
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
                ),
            }
        }
    }
}

pub fn valid_memory_value(kind: &str, value: &serde_json::Value) -> bool {
    match kind {
        "u8" => value.as_u64().is_some_and(|v| u8::try_from(v).is_ok()),
        "u16" => value.as_u64().is_some_and(|v| u16::try_from(v).is_ok()),
        "u32" => value.as_u64().is_some_and(|v| u32::try_from(v).is_ok()),
        "u64" => value.as_u64().is_some(),
        "i32" => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
        "i64" => value.as_i64().is_some(),
        "f32" => value.as_f64().is_some_and(|v| (v as f32).is_finite()),
        "bytes" => match value {
            serde_json::Value::Array(values) => !values.is_empty() && values.iter().all(|v| valid_memory_value("u8", v)),
            serde_json::Value::String(hex) => !hex.trim().is_empty() && hex.split_whitespace().all(|part| u8::from_str_radix(part.trim_start_matches("0x"), 16).is_ok()),
            _ => false,
        },
        _ => false,
    }
}

pub async fn write_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemWriteRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    let pid = match r.pid {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "not_attached", "message": "Game not attached" })),
            )
        }
    };

    let target_addr = match parse_hex_or_dec_address(&payload.address) {
        Some(a) => a,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "status": "invalid_address", "message": "Could not parse address" })),
            )
        }
    };

    if !valid_memory_value(&payload.r#type, &payload.value) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"success": false, "message": "Value is invalid or out of range for the requested type"})));
    }
    let driver = LinuxMemoryDriver::new(pid);

    match payload.r#type.as_str() {
        "u8" => {
            let val = payload.value.as_u64().unwrap_or(0) as u8;
            match driver.write::<u8>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 1 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u16" => {
            let val = payload.value.as_u64().unwrap_or(0) as u16;
            match driver.write::<u16>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 2 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "i32" => {
            let val = payload.value.as_i64().unwrap_or(0) as i32;
            match driver.write::<i32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u32" => {
            let val = payload.value.as_u64().unwrap_or(0) as u32;
            match driver.write::<u32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "i64" => {
            let val = payload.value.as_i64().unwrap_or(0);
            match driver.write::<i64>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 8 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "u64" => {
            let val = payload.value.as_u64().unwrap_or(0);
            match driver.write::<u64>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 8 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "f32" => {
            let val = payload.value.as_f64().unwrap_or(0.0) as f32;
            match driver.write::<f32>(target_addr, &val) {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "success": true, "bytes_written": 4 }))),
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))),
            }
        }
        "bytes" => {
            let bytes_to_write: Vec<u8> = if let Some(arr) = payload.value.as_array() {
                arr.iter().map(|v| v.as_u64().unwrap_or(0) as u8).collect()
            } else if let Some(hex_str) = payload.value.as_str() {
                hex_str
                    .split_whitespace()
                    .filter_map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .collect()
            } else {
                Vec::new()
            };

            if bytes_to_write.is_empty() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "success": false, "message": "No valid bytes to write" })),
                );
            }

            match driver.write_bytes(target_addr, &bytes_to_write) {
                Ok(written) if written == bytes_to_write.len() => (
                    StatusCode::OK,
                    Json(serde_json::json!({ "success": true, "bytes_written": written })),
                ),
                Ok(written) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "success": false, "bytes_written": written, "message": "Incomplete memory write" })),
                ),
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "success": false, "message": e.to_string() })),
                ),
            }
        }
        _ => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "message": "Unsupported data type" })),
        ),
    }
}

pub async fn scan_memory(
    State(state): State<ServerState>,
    Json(payload): Json<MemScanRequest>,
) -> impl IntoResponse {
    let (pid, _ga_base) = {
        let r = state.inner.read().await;
        match r.pid {
            Some(p) => (p, r.game_assembly_base),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "status": "not_attached", "message": "Process not attached" })),
                )
            }
        }
    };

    let mappings = match parse_process_maps(pid) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            )
        }
    };

    let driver = LinuxMemoryDriver::new(pid);
    let mut matches = Vec::new();

    for map in mappings {
        if !map.perms.contains('w') || !map.perms.contains('r') {
            continue;
        }

        let region_len = map.end.saturating_sub(map.start);
        if region_len == 0 || region_len > 32 * 1024 * 1024 {
            continue;
        }

        let mut buf = vec![0u8; region_len.min(512 * 1024)];
        let mut cur_addr = map.start;

        while cur_addr < map.end {
            let to_read = (map.end - cur_addr).min(buf.len());
            if driver.read_bytes(cur_addr, &mut buf[..to_read]).is_err() {
                break;
            }

            match payload.r#type.as_str() {
                "i32" => {
                    if let Ok(target_val) = payload.value.parse::<i32>() {
                        let target_bytes = target_val.to_le_bytes();
                        if to_read >= 4 {
                            for i in (0..to_read - 3).step_by(4) {
                                if buf[i..i + 4] == target_bytes {
                                    let match_addr = cur_addr + i;
                                    matches.push(ScanMatch {
                                        address: format!("0x{:x}", match_addr),
                                        address_val: match_addr,
                                        value: target_val.to_string(),
                                    });
                                    if matches.len() >= 5000 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                "i64" => {
                    if let Ok(target_val) = payload.value.parse::<i64>() {
                        let target_bytes = target_val.to_le_bytes();
                        if to_read >= 8 {
                            for i in (0..to_read - 7).step_by(8) {
                                if buf[i..i + 8] == target_bytes {
                                    let match_addr = cur_addr + i;
                                    matches.push(ScanMatch {
                                        address: format!("0x{:x}", match_addr),
                                        address_val: match_addr,
                                        value: target_val.to_string(),
                                    });
                                    if matches.len() >= 5000 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                "u8" => {
                    if let Ok(target_val) = payload.value.parse::<u8>() {
                        for (i, &byte_val) in buf.iter().enumerate().take(to_read) {
                            if byte_val == target_val {
                                let match_addr = cur_addr + i;
                                matches.push(ScanMatch {
                                    address: format!("0x{:x}", match_addr),
                                    address_val: match_addr,
                                    value: target_val.to_string(),
                                });
                                if matches.len() >= 5000 {
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            if matches.len() >= 5000 {
                break;
            }

            cur_addr += to_read;
        }

        if matches.len() >= 5000 {
            break;
        }
    }

    let total = matches.len();
    {
        let mut w = state.inner.write().await;
        w.scan_results = matches.clone();
        w.last_scan_type = payload.r#type.clone();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "total_matches": total,
            "results": matches.into_iter().take(200).collect::<Vec<_>>()
        })),
    )
}

pub async fn scan_memory_next(
    State(state): State<ServerState>,
    Json(payload): Json<MemScanRequest>,
) -> impl IntoResponse {
    let (pid, prev_matches) = {
        let r = state.inner.read().await;
        match r.pid {
            Some(p) => (p, r.scan_results.clone()),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "status": "not_attached", "message": "Process not attached" })),
                )
            }
        }
    };

    let driver = LinuxMemoryDriver::new(pid);
    let mut filtered = Vec::new();

    for m in prev_matches {
        let addr = m.address_val;
        match payload.r#type.as_str() {
            "i32" => {
                if let Ok(target_val) = payload.value.parse::<i32>() {
                    if let Ok(cur_val) = driver.read::<i32>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            "i64" => {
                if let Ok(target_val) = payload.value.parse::<i64>() {
                    if let Ok(cur_val) = driver.read::<i64>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            "u8" => {
                if let Ok(target_val) = payload.value.parse::<u8>() {
                    if let Ok(cur_val) = driver.read::<u8>(addr) {
                        if cur_val == target_val {
                            filtered.push(ScanMatch {
                                address: m.address,
                                address_val: addr,
                                value: cur_val.to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let total = filtered.len();
    {
        let mut w = state.inner.write().await;
        w.scan_results = filtered.clone();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "success",
            "total_matches": total,
            "results": filtered.into_iter().take(200).collect::<Vec<_>>()
        })),
    )
}

pub async fn heal_action(
    State(state): State<ServerState>,
    Json(payload): Json<HealRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    if let Some(pid) = r.pid {
        let driver = LinuxMemoryDriver::new(pid);
        if payload.player_ptr == 0 || payload.heal_squad {
            return api_error(StatusCode::BAD_REQUEST, "An explicit verified player_ptr is required; use the club preset API for squad actions");
        }
        let player_ptr = payload.player_ptr;

        match SquadActions::heal_player(&driver, player_ptr, payload.condition_offset) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "healed",
                    "player_ptr": format!("0x{:x}", player_ptr),
                    "condition": 10000,
                    "sharpness": 10000
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "not_attached", "message": "Game process not attached" })),
        )
    }
}

pub async fn finances_action(
    State(state): State<ServerState>,
    Json(payload): Json<FinanceRequest>,
) -> impl IntoResponse {
    let r = state.inner.read().await;
    if payload.club_ptr == 0 {
        return api_error(StatusCode::BAD_REQUEST, "An explicit verified club_ptr is required");
    }
    if let Some(pid) = r.pid {
        let driver = LinuxMemoryDriver::new(pid);
        match SquadActions::update_finances(
            &driver,
            payload.club_ptr,
            payload.balance_offset,
            payload.transfer_offset,
            payload.wage_offset,
            payload.balance,
            payload.transfer_budget,
            payload.wage_budget,
        ) {
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "finances_updated",
                    "club_ptr": format!("0x{:x}", payload.club_ptr),
                    "balance": payload.balance,
                    "transfer_budget": payload.transfer_budget,
                    "wage_budget": payload.wage_budget
                })),
            ),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "status": "error", "message": e.to_string() })),
            ),
        }
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "status": "not_attached", "message": "Game process not attached" })),
        )
    }
}

pub async fn godmode_action(
    State(state): State<ServerState>,
    Json(payload): Json<GodModeRequest>,
) -> impl IntoResponse {
    let mut w = state.inner.write().await;
    if payload.player_ptr.is_some() {
        return api_error(StatusCode::BAD_REQUEST, "Select a player through the context API before applying a preset");
    }
    let action = match payload.action.as_str() {
        "max_morale" => "max_morale",
        "cure_injuries" => "heal_player",
        "clear_fatigue" | "unlock_debug_menu" => return api_error(
            StatusCode::NOT_IMPLEMENTED, "This action has no verified memory binding"),
        _ => return api_error(StatusCode::BAD_REQUEST, "Unknown godmode action"),
    };
    match w.context_mgr.apply_quick_action(action) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "success", "player": w.context_mgr.get_context().player}))),
        Err(error) => api_error(StatusCode::BAD_REQUEST, error),
    }
}

pub async fn get_offsets() -> impl IntoResponse {
    let parsed: serde_json::Value =
        serde_json::from_str(DEFAULT_OFFSETS_JSON).unwrap_or(serde_json::json!({}));
    Json(parsed)
}
