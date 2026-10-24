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
if [[ "$1" == "--desktop" || "$1" == "-d" ]]; then
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

# ── 5. Auto-Browser Launcher in Background ────────────────────────────────────
open_browser_when_ready() {
  local target_url="http://localhost:41789"
  local attempts=0
  local max_attempts=50

  while [ $attempts -lt $max_attempts ]; do
    sleep 0.4
    if curl -s -o /dev/null -w "%{http_code}" "$target_url" 2>/dev/null | grep -q "200"; then
      echo -e "\n${BOLD}${GREEN}🚀 FMLE by Vanaila is LIVE at ${target_url}!${RESET}"
      echo -e "${CYAN}Opening live web interface in your default browser...${RESET}\n"
      if command -v xdg-open >/dev/null 2>&1; then
        xdg-open "$target_url" >/dev/null 2>&1 || true
      elif command -v open >/dev/null 2>&1; then
        open "$target_url" >/dev/null 2>&1 || true
      elif command -v sensible-browser >/dev/null 2>&1; then
        sensible-browser "$target_url" >/dev/null 2>&1 || true
      fi
      break
    fi
    attempts=$((attempts + 1))
  done
}

open_browser_when_ready &

# ── 6. Start FMLE Daemon & Live UI ────────────────────────────────────────────
echo -e "\n${BOLD}${GREEN}Starting FMLE by Vanaila daemon on port 41789...${RESET}"
echo -e "${CYAN}Dashboard URL: ${BOLD}http://localhost:41789${RESET}"
echo -e "${YELLOW}Press Ctrl+C at any time to stop the server.${RESET}\n"

# Run optimized release build or build if needed
exec cargo run -p fmlev-cli --release -- serve --port 41789
