```text
  ███████╗███╗   ███╗██╗     ███████╗    ██████╗ ██╗   ██╗
  ██╔════╝████╗ ████║██║     ██╔════╝    ██╔══██╗╚██╗ ██╔╝
  █████╗  ██╔████╔██║██║     █████╗      ██████╔╝ ╚████╔╝ 
  ██╔══╝  ██║╚██╔╝██║██║     ██╔══╝      ██╔══██╗  ╚██╔╝  
  ██║     ██║ ╚═╝ ██║███████╗███████╗    ██████╔╝   ██║   
  ╚═╝     ╚═╝     ╚═╝╚══════╝╚══════╝    ╚═════╝    ╚═╝   
                                                           
   ██╗   ██╗ █████╗ ███╗   ██╗ █████╗ ██╗██╗      █████╗  
   ██║   ██║██╔══██╗████╗  ██║██╔══██╗██║██║     ██╔══██╗ 
   ██║   ██║███████║██╔██╗ ██║███████║██║██║     ███████║ 
   ╚██╗ ██╔╝██╔══██║██║╚██╗██║██╔══██║██║██║     ██╔══██║ 
    ╚████╔╝ ██║  ██║██║ ╚████║██║  ██║██║███████╗██║  ██║ 
     ╚═══╝  ╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝  ╚═╝╚═╝╚══════╝╚═╝  ╚═╝ 
```

<div align="center">

# FMLE by Vanaila

**Real-Time Memory Editor, Scouting Suite & Tactical Toolkit for Football Manager 26**

[![GitHub Release](https://img.shields.io/github/v/release/coderaulia/fmlev?style=for-the-badge&color=emerald)](https://github.com/coderaulia/fmlev/releases/latest)
[![Linux Compatibility](https://img.shields.io/badge/Linux%20Engine-Steam%20Proton%20%7C%20Wine%20%7C%20Native-orange?style=for-the-badge)](https://github.com/coderaulia/fmlev)
[![Open Source](https://img.shields.io/badge/Open%20Source-100%25%20Free-brightgreen?style=for-the-badge)](https://github.com/coderaulia/fmlev)
[![FM26 Engine](https://img.shields.io/badge/FM26%20Engine-Unity%206%20IL2CPP-purple?style=for-the-badge)](https://unity.com)
[![Community](https://img.shields.io/badge/Community-Indonesian%20FM%20Community%20🇮🇩-red?style=for-the-badge)](https://github.com/coderaulia/fmlev)

### 🇮🇩 *Made with love from Jakarta for the Indonesian & Global Football Manager community!*

*An open-source, high-performance real-time memory editor, scouting suite, and in-game toolkit for **Football Manager 26** running on Linux (via Steam Proton / Wine) and Windows.*

</div>

---

## 📸 Overview & Live Dashboard

<p align="center">
  <img src="assets/icon.svg" alt="FMLE by Vanaila Logo" width="160">
</p>

* **⚽ Real-Time Memory Synchronization**: Live zero-copy memory reads and writes (`process_vm_readv` / `process_vm_writev`) directly hooked into FM26's Unity 6 IL2CPP memory space.
* **🌍 Nation Details & Youth Rating** *(offline fixtures; live nation writes not yet verified)*: Modify national youth ratings (1–200), international reputation, economic development, FA financial strength, and corruption metrics.
* **🏟️ Advanced Club Facilities & Board Suite** *(offline fixtures; live club money is verified, see below)*: Full 1–20 facilities matrix (training, youth, recruitment, junior coaching, data analysis), sugar daddy ownership, board patience, and stadium expansion.
* **🤝 Multi-Tier Affiliated Clubs**: Configure bilateral affiliate link terms (first-option buy, free loan wages, shared scouting, work permit havens, annual fees).
* **🏆 Competition Rules & Rules Relaxer** *(offline fixtures; live prize money and register-anytime are verified, rules are not)*: Adjust match substitution limits (e.g. 5 from 12), VAR activation, prize money, TV rights, and foreign player quotas.
* **⚡ Squad God Mode**: One-click condition restoration (100%), match fitness, injury clearance, and morale maximization.
* **📱 Dual Interfaces**: Rich cyberpunk embedded Web UI (`http://localhost:41789`) plus an ultra-fast in-game Dear ImGui & Terminal HUD overlay.

---

Live player writes support positions, foot ratings, all 60 exposed attributes
(technical, mental, physical, goalkeeper and hidden/personality), CA/PA,
and validated weekly wages (GBP) and contract expiry years. Open one Attributes profile in FM26, then use
**Scan Loaded Players** to index native records for name search and editing without
opening every profile. The live scan indexed 49,467 players in the tested save.
Other entity saves return an explicit unsupported error until their native layouts
are verified. See [live verification and limitations](LIVE_READ_WRITE.md).

## 📦 Downloads & Distribution

Get started in seconds! Run the 1-click launcher or install a desktop icon to launch FMLE without ever typing a terminal command.

| Platform / Edition | Package Format | Direct Launcher | Quick Start |
| :--- | :--- | :--- | :--- |
| **Linux (Steam Proton / Wine)** | 1-Click Desktop Icon | `start.sh --desktop` | Double-click **FMLE by Vanaila** on Desktop |
| **Linux (Terminal)** | Bash Shell Script | `./start.sh` | Auto-detects FM26 process & opens browser |
| **Windows 10 / 11** | Batch Script | `start.bat` | Double-click `start.bat` |
| **Binary CLI** | Native Rust Executable | `cargo run -p fmlev-cli` | `cargo run -p fmlev-cli -- serve` |

---

## 🌟 What is FMLE by Vanaila?

**FMLE by Vanaila** is the premier open-source live editor engineered specifically for **Football Manager 26's new Unity 6 engine**. 

FM26 made history by transitioning away from Sports Interactive's legacy proprietary C++ engine to Unity 6 (64-bit IL2CPP). `FMLE` reverse-engineers the IL2CPP metadata v31 runtime, extracting runtime symbol tables, data references, and UI singletons to give you unrestricted control over your save game.

### ✅ Verified Live Write Support (against the running game)

Only fields confirmed in FM26 itself are writable while attached; everything else is shown read-only instead of pretending to save. Each write uses identity validation, readback and rollback.

| Area | What you can do live | Notes |
| --- | --- | --- |
| **Players** | 60 attributes, positions and feet, CA/PA, weekly wage, contract expiry, condition/sharpness heal | Open the player's profile first |
| **Contract extension** | Extend expiry by 1–10 years (`POST /api/player/contract/extend`) | Same date, later year; loaned-out players extend the parent contract; shows after the next day |
| **Contract termination** | Release player as free agent (`POST /api/player/contract/terminate`) | Multi-object detachment: compacts squad vectors, nulls `0x1e8` contract and `0x130`/`0x138` team pointers **UNVERIFIED in game; see AGENTS.md Phase 11** |
| **Free agent signing** | Sign unemployed free agents (`POST /api/transfer/sign`) | Formats aligned 200-byte `FULL_CONTRACT` object, sets RTTI/wage/expiry/team pointers, and appends to target squad **UNVERIFIED in game; see AGENTS.md Phase 11** |
| **Loan recall** | Recall player from loan (`POST /api/player/loan/terminate`) | Compacts loan squad vector, returns team to parent club, clears `+0x1f0` loan node **UNVERIFIED in game; see AGENTS.md Phase 11** |
| **Player transfers** | Move player between clubs (`POST /api/transfer/move`) | Compacts source squad vector, appends to destination team (verifying spare capacity), updates team pointers **UNVERIFIED in game; see AGENTS.md Phase 11** |
| **Pending transfer acceleration** | Accelerate or instantly complete arranged transfers (`POST /api/transfer/accelerate`) | Updates scheduled join dates in future contract and transfer queue, or switches contract immediately **UNVERIFIED in game; see AGENTS.md Phase 11** |
| **Squad numbers** | Set a number (1–99 or none); a teammate wearing it is swapped | Shows in game after the next day |
| **Tactical familiarity** | Max for one player or the whole squad | Same change as the game's own action |
| **Club money & facilities** | Balance, transfer budget, training/youth/coaching/recruitment/data analysis/corporate facilities, sugar daddy status, reputations, stadium capacity | Reopen Club > Finances / Facilities; saved natively via `NativeClubRecord` |
| **Nation macro, youth & population** | Youth rating (1–200), international reputation (1–10,000), economic factor, state of development, population | Native container architecture (`NativeNationRecord`); live slider updates youth rating |
| **Competition reputation** | Native league reputation (1–200 scale) | Saved natively via `NativeCompetitionRecord` |
| **Competition stage rules** | Dynamic registration squad cap (`POST /api/competition/stage-rules`) | Queries and edits active simulation engine `stage_rule_entry` (`+0x98`/`+0xd6`) |
| **Competition prize money** | Edit the per-position prize table | Match first/last prize on Rules; payout source at season end unconfirmed |
| **Register anytime** | Allow squad registration outside the window per competition | Toggles manager flag bit `0x100`, manager squad-number bit `0x200`, and domestic stage gate byte `0x9a=1` |
| **Reads** | Club, nation and competition overviews, FIFA ranking, reputation text | Screen-independent discovery via native records |

**Remaining frontiers & limits**:
1. **Creating loans from scratch**: Recalling loans is fully supported (`/api/player/loan/terminate`), but establishing a brand-new loan agreement between two clubs from scratch requires instantiating the game's complex `LOAN_CONTRACT` object graph with multi-party clauses.
2. **In-match simulation rules**: Dynamic stage registration caps are writable, but matchday in-play rules (substitutions count, bench slots, VAR toggles) reside in static competition rule blueprints.
3. **Unmapped club & nation fields**: Club board patience and fan confidence remain unmapped. Nation tax rate, corruption index, languages, and FIFA world ranking points are unmapped.

> ⚠️ **Never edit a player's injury vector.** An emptied vector for an injured player crashes FM26 on the next day and the damage is saved into the career.
> Always test on a throwaway save, and Save As a new file after live edits.

The server only accepts requests from `localhost` / `127.0.0.1` pages (foreign `Origin`/`Host` headers are rejected), because every route can write game memory.

**Diagnostics:** `tools/fmscan` is a read-only C++ memory scanner for differential investigation and screen-independent native entity discovery. `entity PID club|nation|competition --name TEXT --editor --json` reads guarded facilities, finance, grounds, gender-specific nation youth/reputation and competition reputation/stage-gate data. `fmwrite` is a standalone journaled CLI writer for test-save plan/apply/restore experiments. See [exact native field coverage and remaining WIP](docs/NATIVE_ENTITY_READS.md) and [scanner usage](tools/fmscan/README.md).

### Core Capabilities:

1. **⚽ Player Attributes & Hidden Ratings Editor**:
   - Technical, Mental, Physical, and Goalkeeping attributes (1–20).
   - Hidden attributes: Consistency, Important Matches, Injury Proneness, Versatility, Ambition, Loyalty, Pressure, Professionalism, Sportsmanship, and Temperament.
   - CA (Current Ability: 1–200) and PA (Potential Ability: 1–200).
   - Position versatility matrix (GK, DL, DC, DR, WBL, WBR, DM, ML, MC, MR, AML, AMC, AMR, ST) and foot ratings.
   - Contract editor: Weekly wages, contract expiry year, release clauses, squad status, and happiness.

2. **🌍 Nation Details & Regen Youth Rating**:
   - **Youth Rating (1–200)**: The single most influential parameter determining the quality of regenerated wonderkids (newgens).
   - **Macro Metrics**: International reputation (1–10,000), economic factor (1–20), state of development (Developed / Developing / Third World), FA financial power (1–20), tax rate (0–100%), and game importance.
   - **Corruption Index (0–20)**: Simulates match-fixing and governance scandal tendencies.
   - **⭐ Golden Generation Preset**: Instantly elevates a nation's youth rating to 200/200 and youth facilities to 20/20.

3. **🏟️ Club Facilities, Board & Sugar Daddy Suite**:
   - **Facilities Matrix (1–20)**: Training Facilities, Youth Facilities, Youth Recruitment, Junior Coaching, Data Analysis Facilities, and Corporate / VIP Suites.
   - **Board & Ownership**: Sugar Daddy status (`None`, `FrontEnd`, `Underwriter`, `Foreground`), Board Patience (1–20), Fan Confidence (1–100%), Board Confidence (1–100%), and Transfer Revenue Retention % (0–100%).
   - **Finances & Stadium**: Transfer budget, wage budget, overall balance, stadium capacity, and expansion limits.

4. **🤝 Affiliated Clubs Manager**:
   - Manage feeder clubs and parent club partnerships.
   - Configure bilateral link terms: *First Option to Buy*, *Send Players on Loan (100% wage coverage)*, *Shared Scouting Knowledge*, *Work Permit Havens*, and annual financial fees.

5. **🏆 Competition Rules & Prize Pools**:
   - In-match rules: Max substitutes allowed (e.g. 5), bench size (e.g. 9 or 12), stoppage windows (e.g. 3).
   - VAR toggle (`Active` / `Disabled`).
   - Registration restrictions: Maximum foreign / non-EU players, squad size limit, and homegrown player quotas.
   - Financial awards: Winner prize money and TV rights pool distributions.
   - **⚡ Relax Rules Preset**: Removes foreign player caps (99 max) and unlocks 12 substitutes on the bench.

6. **🔍 4-Way Omni-Search**:
   - Universal search bar supporting instant auto-complete across **Players**, **Clubs**, **Nations**, and **Competitions**.

---

## 🚀 1-Click Automated Quick Start (For Daily Users)

No need to open a terminal every time you play! You can create a native desktop icon just like a regular game or application.

### 🐧 Linux (Steam Proton / Steam Deck / Native Distros)

#### Step 1: Create Desktop Shortcut (Run Once)
Open your terminal in the `fmlev` folder and run:
```bash
./start.sh --desktop
```
This automatically:
- Creates an application menu launcher at `~/.local/share/applications/fmle-vanaila.desktop`.
- Creates a double-clickable Desktop shortcut at `~/Desktop/fmle-vanaila.desktop`.
- Configures icon and permissions.

#### Step 2: Daily Use (No Commands Needed!)
1. Launch **Football Manager 26** in Steam.
2. Double-click the **FMLE by Vanaila** icon on your Desktop (or open it from your Application Menu / Steam Deck desktop mode).
3. FMLE builds the current code, keeps its terminal open, and opens your browser at `http://localhost:41789`. Ctrl+C stops the server.
4. Edit your players, club, nation, or competitions on the fly!

Server controls (Linux):

```bash
./start.sh                  # Start; an existing server is reused, never duplicated
./start.sh status           # PID, binary build time, and whether it is outdated
./start.sh restart          # Build first, then replace the server on port 41789
./start.sh stop             # Stop the server, including older launcher instances
./start.sh restart --background  # Optional detached mode with a persistent log
```

Normal starts keep the server in the terminal, including when reusing an existing
server. Ctrl+C or closing that terminal stops the server. Restart rebuilds the code and
opens the browser only after the replacement is ready. If the build fails, the
existing server stays running. Background output goes to
`${XDG_STATE_HOME:-~/.local/state}/fmlev/server.log`; use `stop` from any terminal
to terminate it. An unrelated application on the port is never terminated.

> **💡 Note on Linux Yama Ptrace Permissions**:
> Modern Linux distributions protect processes with Yama LSM. If FMLE indicates that memory access is restricted, run:
> ```bash
> sudo sysctl -w kernel.yama.ptrace_scope=0
> ```

---

### 🪟 Windows (10 / 11)

1. Launch **Football Manager 26**.
2. Double-click `start.bat`.
3. Your browser will automatically open to `http://localhost:41789`.

---

## 📖 Manual Developer Setup

If you are developing or customizing FMLE:

```bash
# 1. Clone the repository
git clone https://github.com/coderaulia/fmlev.git
cd fmlev

# 2. Run test suite
cargo test --workspace

# 3. Start live editor RPC server on custom port
cargo run -p fmlev-cli -- serve --port 41789
```

Open `http://localhost:41789` in any web browser.

---

## ⌨️ CLI Command Reference

`fmlev` provides a high-performance command-line interface for diagnostics and automation:

```bash
# Check running FM26 process, Proton Wine-prefix, and Linux Yama capabilities
cargo run -p fmlev-cli -- status

# Inspect local FM26 IL2CPP metadata v31 structures
cargo run -p fmlev-cli -- inspect-meta

# Start the embedded live editor daemon
cargo run -p fmlev-cli -- serve --port 41789

# Run tests across all crates (fmlev-core, fmlev-server, fmlev-mem, fmlev-il2cpp, fmlev-overlay)
cargo test --workspace
```

---

## 📁 Repository Structure

```
fmlev/
├── Cargo.toml                  # Workspace manifest
├── start.sh                    # 1-Click launcher & desktop shortcut installer
├── start.bat                   # Windows 1-Click launcher
├── assets/                     # Application icons (icon.svg, icon.png)
├── offsets/
│   └── v26.0.0.json            # Patch-resilient IL2CPP offset mappings
├── crates/
│   ├── fmlev-mem/              # Zero-copy Linux memory driver (process_vm_readv/writev)
│   ├── fmlev-il2cpp/           # IL2CPP types, singleton reader, metadata parser
│   ├── fmlev-core/             # Domain models (Player, Club, Nation, Competition)
│   ├── fmlev-server/           # Embedded Axum REST/WebSocket server + embedded UI
│   ├── fmlev-overlay/          # Dear ImGui in-game overlay & Terminal HUD controller
│   └── fmlev-cli/              # CLI executable (fmlev status, fmlev serve)
├── output_dump/                # Decompiled C# structures (dump.cs, script.json)
└── tools/                      # IL2CPP Dumper extraction utilities
```

---

## 🤝 Open Source & Community Contribution

**FMLE by Vanaila** is an **open-source project built for the global football gaming community, with special love for the Indonesian FM & tech ecosystem 🇮🇩**.

Contributions are warmly welcomed:
- 💡 **Report Issues & Bugs**: Help test memory bindings on Steam Deck, Proton Experimental, and Windows.
- 🎯 **Submit Offsets**: Help maintain `offsets/v*.json` for new FM26 patch cycles.
- 💻 **Submit Pull Requests**: Add new tactical macros, scouting views, or UI translations.

To get in touch or collaborate, connect via GitHub or email [care@vanaila.com](mailto:care@vanaila.com).

---

## 📜 License & Credits

- Built with ❤️ using Rust, Axum, Dear ImGui, and modern Web Components.
- Reverse-engineering research assisted by Il2CppDumper.
- Distributed under the MIT License.

<div align="center">

**[⭐ Star this project on GitHub](https://github.com/coderaulia/fmlev)** • **Made with ❤️ from Jakarta for the Indonesian & Global FM Community! 🇮🇩**

</div>

## Using the editor inside the game (Steam overlay)

No extra tooling is needed if FM26 is launched through Steam with the in-game overlay enabled:

1. Start the server and attach: `./target/release/fmlev-cli serve --port 41789`, then attach to the running game.
2. In the game press **Shift+Tab** to open the Steam overlay.
3. Open the overlay **Web Browser** and go to `http://127.0.0.1:41789` (add `?mode=overlay` for the compact layout). Bookmark it in the overlay browser.
4. Use the editor, then press Shift+Tab again to return to the game.

Requirements: Steam Settings > In Game > "Enable the Steam Overlay while in-game" (also for non-Steam games if launched that way). If the overlay does not appear under Proton, check the game's launch options and that the Steam overlay works for other games. See `docs/OVERLAY_PLAN.md` for custom-hotkey options.

