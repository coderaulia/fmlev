# Plan: open the FMLE web UI over the running game (no alt-tab)

Status: plan only, nothing implemented. Target: Linux, FM26 under Proton, fmlev server on `127.0.0.1:41789`.
Current state: `fmlev overlay` opens `?mode=overlay` in a Chromium-family app window (`crates/fmlev-overlay/src/launcher.rs`); it is a normal window, so it needs alt-tab.

## Constraints
- The overlay must only appear after the server answers `GET /api/status` (server running and attached to the game).
- Never inject into or hook the game process (Proton/Wine DLL injection risks crashes, saves and anti-cheat flags, and breaks AGENTS.md rule 10). The overlay stays a separate process.
- Hotkey must work while the game has focus, and must not be a key the game uses.

## Options (cheapest first)

| # | Approach | Effort | In-game over fullscreen? | Notes |
| --- | --- | --- | --- | --- |
| A | **Steam overlay browser.** Launch FM26 from Steam, press Shift+Tab, open `http://127.0.0.1:41789` in Steam's web browser. | none | yes | Works today if the game runs through Steam with the overlay enabled. Pin the URL in the overlay browser favourites. No hotkey of our own. |
| B | **Always-on-top app window + global hotkey (borderless game).** Run the game in borderless/windowed. A small daemon (`fmlev overlay --daemon`) registers a global hotkey and shows/hides a Chromium `--app` window flagged always-on-top. | low | borderless only | Hotkey via the XDG GlobalShortcuts portal (works on COSMIC/KDE/GNOME Wayland) with an evdev fallback (`/dev/input`, needs `input` group). Always-on-top on Wayland needs compositor support (layer-shell or a window rule); on X11 use `_NET_WM_STATE_ABOVE`. |
| C | **Gamescope wrapper.** Start the game inside `gamescope` (installed here) and show the UI as a second client/overlay layer. | medium | yes | Changes how the game is launched (Steam launch options). Gamescope input routing for a second client is fiddly; prototype first. |
| D | **Layer-shell panel (Wayland).** Native wlr-layer-shell surface (overlay layer) hosting a WebView (`wry`/WebKitGTK) with keyboard-interactivity toggled by the hotkey. | medium | yes on compositors with layer-shell (COSMIC supports it) | Best UX on this machine: appears over fullscreen games, no window-manager tricks. Rust: `smithay-client-toolkit` + `wry`, or `gtk-layer-shell`. |
| E | **In-process ImGui/DirectX hook.** What the README calls the ImGui overlay. | high | yes | Rejected: injection into the Proton process. |

## Recommended path
1. **Now (zero code):** use option A. Document it in the README.
2. **Phase 1, readiness gate:** `fmlev overlay --wait` polls `/api/status` (attached + `game_assembly_base` present) with a timeout, then opens the UI. Also a small `GET /api/overlay/ready` JSON for scripts. Tests: Rust unit test for the poll loop with a fake server.
3. **Phase 2, hotkey toggle (option B):** `fmlev overlay --daemon --hotkey "Ctrl+Alt+F"` (configurable, default avoids Shift+Tab and Escape). Toggle = show/hide the app window; hide keeps the page loaded so state (selected player, dirty form) survives. Verify on COSMIC: portal availability, always-on-top behaviour over a borderless game.
4. **Phase 3, true overlay (option D):** layer-shell WebView window shown only while the hotkey is toggled on, grabbing keyboard focus only while visible and returning it to the game on hide. Compact layout (`?mode=overlay`) with a larger click target size for mouse use.
5. Verification per phase: server not running -> overlay refuses to open with a clear message; server running but game detached -> overlay shows "attach" state; hotkey works with game focused; game input (typing, mouse) is not swallowed while the overlay is hidden; no crash or focus loss after 30 minutes of play on a throwaway save.

## Open questions
- Does the game run fullscreen-exclusive or borderless under Proton here? (Only borderless is guaranteed to allow option B.)
- Which hotkey does the user want as default?
- Is Steam-launch required, or may the game be started through `gamescope`?
