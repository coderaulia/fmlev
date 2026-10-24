use clap::{Parser, Subcommand};
use std::path::PathBuf;

use fmlev_il2cpp::metadata::MetadataHeader;
use fmlev_mem::{
    find_fm_process, find_module_bases, get_yama_diagnostic, LinuxMemoryDriver,
};
use fmlev_overlay::{OverlayLauncher, OverlayRenderer};

mod probe;

#[derive(Parser)]
#[command(name = "fmlev")]
#[command(about = "Football Manager 26 Open-Source Live Editor & Toolkit for Linux")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check connection status to Football Manager 26 and Linux ptrace permissions
    Status,

    /// Inspect local global-metadata.dat binary header and version
    InspectMeta {
        #[arg(
            short,
            long,
            default_value = "/home/asw/.local/share/Steam/steamapps/common/Football Manager 26/fm_Data/il2cpp_data/Metadata/global-metadata.dat"
        )]
        metadata_path: PathBuf,
    },

    /// Start the local live editor server daemon and web UI
    Serve {
        #[arg(short, long, default_value_t = 41789)]
        port: u16,
    },

    /// Start the server and automatically launch the compact in-game overlay HUD
    Overlay {
        #[arg(short, long, default_value_t = 41789)]
        port: u16,
    },

    /// Read and hex-dump live memory from running FM26 process
    Read {
        /// Target memory address in hex (e.g. 0x18218e590) or decimal
        hex_addr: String,

        /// Number of bytes to read
        #[arg(default_value_t = 64)]
        size: usize,
    },

    /// Write value directly to running FM26 memory
    Write {
        /// Target memory address in hex (e.g. 0x18218e590)
        hex_addr: String,

        /// Data type: u8, u16, i32, i64, f32, or bytes
        #[arg(value_name = "TYPE")]
        data_type: String,

        /// Value to write (e.g. 200, 15000000, or "90 90" for bytes)
        value: String,
    },

    /// Read-only diagnostic: dump every live BindingSubsystem entry (property, TypedValue class, raw bytes, decodes)
    ProbeBindings {
        /// Only show references whose Name/FullName/InitialSurname/ShirtName contains this substring
        #[arg(long)]
        person: Option<String>,

        /// Only show properties whose name contains this substring
        #[arg(long)]
        prop: Option<String>,

        /// Maximum number of references (persons) to print
        #[arg(long, default_value_t = 5)]
        limit: usize,

        /// Include non-person references (SourceRef +0x24 != 1); m_index is then (type<<24 | index)
        #[arg(long)]
        all_types: bool,
    },

    /// Read-only diagnostic: print an IL2CPP object's class and fields (qwords), recursing into pointers
    ProbeObject {
        /// Object address (hex)
        hex_addr: String,
        /// Bytes to dump per object
        #[arg(long, default_value_t = 0x40)]
        size: usize,
        /// Recursion depth into referenced objects
        #[arg(long, default_value_t = 1)]
        depth: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            println!("=== Football Manager 26 Live Editor: Status Check ===");
            println!("Checking Linux security policies...");
            println!("{}\n", get_yama_diagnostic());

            println!("Searching for active Football Manager 26 process...");
            match find_fm_process() {
                Ok(proc) => {
                    println!("  [+] Found FM process: PID {}", proc.pid);
                    println!("  [+] Binary: {:?}", proc.exe_path);
                    println!("  [+] Proton/Wine: {}", proc.is_proton);
                    if let Some(pfx) = proc.wine_prefix {
                        println!("  [+] Wine Prefix: {:?}", pfx);
                    }

                    match find_module_bases(proc.pid) {
                        Ok(bases) => {
                            if let Some(b) = bases.game_assembly_base {
                                println!("  [+] GameAssembly.dll Base: 0x{:x}", b);

                                // Test live memory read with our LinuxMemoryDriver!
                                let driver = LinuxMemoryDriver::new(proc.pid);
                                let mut magic = [0u8; 2];
                                match driver.read_bytes(b, &mut magic) {
                                    Ok(_) => {
                                        let magic_str = String::from_utf8_lossy(&magic);
                                        println!(
                                            "  [+] Live Memory Read: Verified PE Header Magic '{magic_str}' at 0x{:x} via process_vm_readv!",
                                            b
                                        );
                                    }
                                    Err(e) => println!("  [-] Memory Read Test Failed: {}", e),
                                }
                            } else {
                                println!("  [-] GameAssembly.dll not yet mapped into memory.");
                            }
                            if let Some(b) = bases.unity_player_base {
                                println!("  [+] UnityPlayer.dll Base: 0x{:x}", b);
                            }
                            if let Some(b) = bases.fm_exe_base {
                                println!("  [+] fm.exe Base: 0x{:x}", b);
                            }
                            if let Ok(modules) = fmlev_mem::find_loaded_modules(proc.pid) {
                                for m in &modules {
                                    if m.name.eq_ignore_ascii_case("game_plugin.dll") {
                                        println!("  [+] game_plugin.dll Base: 0x{:x} (Size: {:.1} MB - Native Sim Engine)", m.base_address, m.size as f64 / (1024.0 * 1024.0));
                                    }
                                }
                            }

                            // Resolve live IL2CPP Singletons from offsets/v26.0.0.json
                            if let (Some(ga_base), Ok(reg)) = (
                                bases.game_assembly_base,
                                fmlev_il2cpp::OffsetsRegistry::load_from_file("offsets/v26.0.0.json"),
                            ) {
                                if let Some(profile) = &reg.active_profile {
                                    println!("\n=== Live IL2CPP Singleton Instances (v{}) ===", profile.version);
                                    let driver = LinuxMemoryDriver::new(proc.pid);
                                    let mut names: Vec<_> = profile.singletons.keys().collect();
                                    names.sort();
                                    for name in names {
                                        if let Some(s_def) = profile.singletons.get(name) {
                                            match fmlev_il2cpp::types::read_static_singleton(&driver, ga_base, s_def) {
                                                Ok(ptr) if ptr != 0 => {
                                                    println!("  [+] {:<20} -> Live Instance @ 0x{:x} (RVA {})", name, ptr, s_def.rva);
                                                }
                                                Ok(_) => {
                                                    println!("  [~] {:<20} -> Null / Uninitialized (RVA {})", name, s_def.rva);
                                                }
                                                Err(e) => {
                                                    println!("  [-] {:<20} -> Resolution error: {}", name, e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => println!("  [-] Could not parse /proc/{}/maps: {}", proc.pid, e),
                    }
                }
                Err(_) => {
                    println!("  [-] Football Manager 26 is not currently running.");
                    println!("      Launch the game via Steam, then run `fmlev status` again.");
                }
            }
        }

        Commands::InspectMeta { metadata_path } => {
            println!("=== Inspecting IL2CPP Metadata: {:?} ===", metadata_path);
            match MetadataHeader::read_from_file(&metadata_path) {
                Ok(hdr) => {
                    println!("  [+] Magic: 0x{:x} (Valid IL2CPP Metadata)", hdr.magic);
                    println!("  [+] Metadata Version: {}", hdr.version);
                    println!("  [+] String Literals Table Size: {} bytes", hdr.string_literal_size);
                    println!("  [+] String Table Size: {} bytes", hdr.string_size);
                    println!("  [+] Methods Table Size: {} bytes", hdr.methods_size);
                    println!("  [+] Type Definitions Table Size: {} bytes", hdr.type_definitions_size);
                }
                Err(e) => println!("  [-] Failed to parse metadata: {}", e),
            }
        }

        Commands::Serve { port } => {
            println!("╔══════════════════════════════════════════════════════════════════════════════╗");
            println!("║              FMLEV 26 - LIVE EDITOR & IN-GAME OVERLAY SERVER                 ║");
            println!("╚══════════════════════════════════════════════════════════════════════════════╝");
            println!("\n  ➜ Live Web Editor UI:  http://127.0.0.1:{}", port);
            println!("  ➜ Compact Overlay HUD: http://127.0.0.1:{}?mode=overlay\n", port);
            println!("Ready! Open the URL above in any browser, or run `fmlev overlay` for auto-HUD.");

            fmlev_server::start_server(port).await?;
        }

        Commands::Overlay { port } => {
            println!("╔══════════════════════════════════════════════════════════════════════════════╗");
            println!("║          FMLEV 26 - LAUNCHING IN-GAME COMPACT HUD OVERLAY (LINUX)            ║");
            println!("╚══════════════════════════════════════════════════════════════════════════════╝\n");

            // Spawn server in background
            tokio::spawn(async move {
                if let Err(e) = fmlev_server::start_server(port).await {
                    eprintln!("[-] Server error: {}", e);
                }
            });

            // Small pause for listener binding
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;

            // Attempt launching overlay window
            match OverlayLauncher::launch(port) {
                Ok(child) => {
                    println!("  [+] Launched Overlay Window (PID: {})", child.id());
                    println!("{}", OverlayLauncher::window_manager_tips());
                }
                Err(e) => {
                    println!("  [!] Could not auto-launch browser window: {}", e);
                    println!("      Open this URL manually: http://127.0.0.1:{}?mode=overlay", port);
                }
            }

            // Print Terminal HUD
            let mut overlay = OverlayRenderer::new();
            if let Ok(proc) = find_fm_process() {
                let bases = find_module_bases(proc.pid).unwrap_or(fmlev_mem::ModuleBases {
                    game_assembly_base: None,
                    unity_player_base: None,
                    fm_exe_base: None,
                });
                overlay.state.update_connection(
                    proc.pid,
                    bases.game_assembly_base,
                    bases.unity_player_base,
                    proc.wine_prefix.map(|p| p.to_string_lossy().to_string()),
                );
            }
            println!("{}", overlay.render_terminal());

            // Keep main task alive
            tokio::signal::ctrl_c().await?;
            println!("\nExiting fmlev overlay.");
        }

        Commands::Read { hex_addr, size } => {
            let addr = parse_address(&hex_addr)?;
            let proc = find_fm_process().map_err(|e| {
                anyhow::anyhow!("Football Manager 26 is not running. Launch the game first! ({})", e)
            })?;

            let driver = LinuxMemoryDriver::new(proc.pid);
            let mut buf = vec![0u8; size.clamp(1, 65536)];

            match driver.read_bytes(addr, &mut buf) {
                Ok(bytes_read) => {
                    println!("=== Live Memory Read at 0x{:x} (PID {}) ===", addr, proc.pid);
                    print_hex_dump(addr, &buf[..bytes_read]);
                }
                Err(e) => {
                    eprintln!("[-] Read failed at 0x{:x}: {}", addr, e);
                }
            }
        }

        Commands::Write {
            hex_addr,
            data_type,
            value,
        } => {
            let addr = parse_address(&hex_addr)?;
            let proc = find_fm_process().map_err(|e| {
                anyhow::anyhow!("Football Manager 26 is not running. Launch the game first! ({})", e)
            })?;

            let driver = LinuxMemoryDriver::new(proc.pid);

            println!("=== Live Memory Write at 0x{:x} (PID {}) ===", addr, proc.pid);

            match data_type.to_lowercase().as_str() {
                "u8" => {
                    let val: u8 = value.parse()?;
                    let prev: Result<u8, _> = driver.read(addr);
                    driver.write(addr, &val)?;
                    println!("  [+] Successfully wrote u8 {} to 0x{:x} (Previous: {:?})", val, addr, prev.ok());
                }
                "u16" => {
                    let val: u16 = value.parse()?;
                    let prev: Result<u16, _> = driver.read(addr);
                    driver.write(addr, &val)?;
                    println!("  [+] Successfully wrote u16 {} to 0x{:x} (Previous: {:?})", val, addr, prev.ok());
                }
                "i32" => {
                    let val: i32 = value.parse()?;
                    let prev: Result<i32, _> = driver.read(addr);
                    driver.write(addr, &val)?;
                    println!("  [+] Successfully wrote i32 {} to 0x{:x} (Previous: {:?})", val, addr, prev.ok());
                }
                "i64" => {
                    let val: i64 = value.parse()?;
                    let prev: Result<i64, _> = driver.read(addr);
                    driver.write(addr, &val)?;
                    println!("  [+] Successfully wrote i64 {} to 0x{:x} (Previous: {:?})", val, addr, prev.ok());
                }
                "f32" => {
                    let val: f32 = value.parse()?;
                    let prev: Result<f32, _> = driver.read(addr);
                    driver.write(addr, &val)?;
                    println!("  [+] Successfully wrote f32 {} to 0x{:x} (Previous: {:?})", val, addr, prev.ok());
                }
                "bytes" => {
                    let bytes: Vec<u8> = value
                        .split_whitespace()
                        .filter_map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                        .collect();

                    if bytes.is_empty() {
                        return Err(anyhow::anyhow!("No valid hex bytes parsed from input: {}", value));
                    }

                    driver.write_bytes(addr, &bytes)?;
                    println!("  [+] Successfully wrote {} raw bytes to 0x{:x}", bytes.len(), addr);
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unsupported type '{}'. Supported: u8, u16, i32, i64, f32, bytes",
                        data_type
                    ));
                }
            }
        }

        Commands::ProbeBindings { person, prop, limit, all_types } => {
            probe::run(probe::ProbeArgs { person, prop, limit, all_types })?;
        }

        Commands::ProbeObject { hex_addr, size, depth } => {
            probe::inspect_object(parse_address(&hex_addr)?, size, depth)?;
        }
    }

    Ok(())
}

fn parse_address(s: &str) -> anyhow::Result<usize> {
    let clean = s.trim().trim_matches('"');
    if let Some(stripped) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        usize::from_str_radix(stripped, 16)
            .map_err(|e| anyhow::anyhow!("Invalid hex address '{}': {}", s, e))
    } else if clean.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        usize::from_str_radix(clean, 16)
            .map_err(|e| anyhow::anyhow!("Invalid hex address '{}': {}", s, e))
    } else {
        clean
            .parse::<usize>()
            .map_err(|e| anyhow::anyhow!("Invalid address '{}': {}", s, e))
    }
}

fn print_hex_dump(base_addr: usize, data: &[u8]) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let addr = base_addr + (i * 16);
        let mut hex_part = String::new();
        let mut ascii_part = String::new();

        for (j, &byte) in chunk.iter().enumerate() {
            hex_part.push_str(&format!("{:02x} ", byte));
            if j == 7 {
                hex_part.push(' ');
            }
            if (32..=126).contains(&byte) {
                ascii_part.push(byte as char);
            } else {
                ascii_part.push('.');
            }
        }

        // Padding for incomplete chunks
        let remaining = 16 - chunk.len();
        if remaining > 0 {
            for _ in 0..remaining {
                hex_part.push_str("   ");
            }
            if chunk.len() <= 8 {
                hex_part.push(' ');
            }
        }

        println!("0x{:012x}:  {:<49} |{}|", addr, hex_part, ascii_part);
    }
}
