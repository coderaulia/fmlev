#!/usr/bin/env bash
# ==============================================================================
# FMLE by Vanaila - Automated Setup & Launch Script (Linux / macOS)
# Real-Time Memory Editor, Scouting Suite & Tactical Toolkit for Football Manager 26
# ==============================================================================

set -e

# Color definitions
BOLD="\033[1m"
GREEN="\033[0;32m"
BLUE="\033[0;34m"
YELLOW="\033[1;33m"
RED="\033[0;31m"
CYAN="\033[0;36m"
MAGENTA="\033[0;35m"
RESET="\033[0m"

APP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$APP_DIR"

echo -e "${BOLD}${GREEN}"
cat << "EOF"
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
EOF
echo -e "${RESET}"
echo -e "${BOLD}${CYAN}FMLE by Vanaila - Football Manager 26 Live Editor & Scouting Suite${RESET}"
echo -e "${BOLD}100% Free & Open-Source • Built with ❤️ for the Indonesian & Global FM Community 🇮🇩${RESET}"
echo "----------------------------------------------------------------------------------"

# ── 1. Desktop shortcut creation argument check ────────────────────────────────
if [[ "${1:-}" == "--desktop" || "${1:-}" == "-d" ]]; then
  echo -e "${BLUE}ℹ Creating Application Menu & Desktop launcher...${RESET}"
  DESKTOP_DIR="${XDG_DESKTOP_DIR:-$HOME/Desktop}"
  APPS_DIR="$HOME/.local/share/applications"

  ICON_PATH="$APP_DIR/assets/icon.png"
  DESKTOP_ENTRY="[Desktop Entry]
Version=1.0
Type=Application
Name=FMLE by Vanaila
GenericName=Football Manager 26 Live Editor
Comment=Real-Time Memory Editor, Scouting Suite & God Mode Toolkit
Exec=\"$APP_DIR/start.sh\"
Icon=$ICON_PATH
Path=$APP_DIR
Terminal=true
Categories=Game;Utility;Development;
StartupNotify=true
Keywords=FM;Football Manager;Editor;Scout;Vanaila;"

  # Linux Application Menu (~/.local/share/applications)
  if mkdir -p "$APPS_DIR" 2>/dev/null; then
    echo "$DESKTOP_ENTRY" > "$APPS_DIR/fmle-vanaila.desktop" 2>/dev/null || true
    chmod +x "$APPS_DIR/fmle-vanaila.desktop" 2>/dev/null || true
    echo -e "${GREEN}✓ Linux Application menu shortcut created at: $APPS_DIR/fmle-vanaila.desktop${RESET}"
  fi

  # Desktop Shortcut
  if [ -d "$DESKTOP_DIR" ] && [ -w "$DESKTOP_DIR" ]; then
    echo "$DESKTOP_ENTRY" > "$DESKTOP_DIR/fmle-vanaila.desktop" 2>/dev/null || true
    chmod +x "$DESKTOP_DIR/fmle-vanaila.desktop" 2>/dev/null || true
    
    # Mark as trusted on GNOME / KDE if gio is available
    if command -v gio >/dev/null 2>&1; then
      gio set "$DESKTOP_DIR/fmle-vanaila.desktop" metadata::trusted true 2>/dev/null || true
    fi
    echo -e "${GREEN}✓ Desktop icon created at: $DESKTOP_DIR/fmle-vanaila.desktop${RESET}"
    echo -e "${CYAN}💡 You can now launch FMLE simply by double-clicking the Desktop icon!${RESET}"
  fi
  echo ""
  exit 0
fi

# Lifecycle commands do not need Rust setup or memory-permission checks.
case "${1:-start}" in
  start|restart|stop|status|--background|--help|-h) ;;
  *) echo "Usage: $0 [start|restart|stop|status] [--background] | --desktop" >&2; exit 2 ;;
esac
if [[ "${1:-}" == "stop" || "${1:-}" == "status" || "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  exec python3 "$APP_DIR/tools/server_control.py" "$@"
fi

# ── 2. Distro & OS Detection ──────────────────────────────────────────────────
detect_distro() {
  if [ -f /etc/os-release ]; then
    . /etc/os-release
    echo "$ID"
  elif command -v uname >/dev/null 2>&1 && [ "$(uname)" = "Darwin" ]; then
    echo "macos"
  else
    echo "unknown"
  fi
}

DISTRO=$(detect_distro)

# ── 3. Check Rust & Cargo Installation ────────────────────────────────────────
check_rust() {
  if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Rust toolchain detected ($(rustc --version)).${RESET}"
    return 0
  fi

  echo -e "${YELLOW}⚠ Rust toolchain not detected.${RESET}"
  echo -e "${CYAN}Installing Rust via official rustup script...${RESET}"
  read -p "Would you like to install Rust automatically now? [Y/n] " -n 1 -r
  echo ""
  if [[ $REPLY =~ ^[Nn]$ ]]; then
    echo -e "${RED}Rust is required to build and run FMLE. Please install it from https://rustup.rs${RESET}"
    exit 1
  fi

  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
  echo -e "${GREEN}✓ Rust installed successfully!${RESET}"
}

check_rust

# ── 4. Linux Memory Permissions (Yama LSM / ptrace) ───────────────────────────
check_yama_ptrace() {
  if [ -f /proc/sys/kernel/yama/ptrace_scope ]; then
    local scope
    scope=$(cat /proc/sys/kernel/yama/ptrace_scope 2>/dev/null || echo "1")
    if [ "$scope" -gt 0 ]; then
      echo -e "${YELLOW}ℹ Linux Yama ptrace_scope is set to $scope (Restricted).${RESET}"
      echo -e "${CYAN}To allow FMLE to inspect Football Manager 26 running in Steam Proton / Wine,${RESET}"
      echo -e "${CYAN}you can run: ${BOLD}sudo sysctl -w kernel.yama.ptrace_scope=0${RESET}"
    else
      echo -e "${GREEN}✓ Linux ptrace permissions OK (scope=0). Ready for live FM26 memory access.${RESET}"
    fi
  fi
}

check_yama_ptrace

# ── 5. Build, own the listener, and keep the terminal attached ─────────────────
if [[ "$(uname -s)" != "Linux" ]]; then
  echo "Server lifecycle commands currently require Linux." >&2
  exit 1
fi
exec python3 "$APP_DIR/tools/server_control.py" "$@"
