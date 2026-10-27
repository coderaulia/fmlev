#!/usr/bin/env python3
"""Own the local Linux editor listener, including servers from older launchers."""
import argparse
import fcntl
import os
from pathlib import Path
import signal
import select
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target/release/fmlev-cli"
PORT = 41789
URL = f"http://127.0.0.1:{PORT}"
STATE = Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state"))) / "fmlev"


def listener_inodes():
    inodes = set()
    for table in ("/proc/net/tcp", "/proc/net/tcp6"):
        for line in Path(table).read_text().splitlines()[1:]:
            fields = line.split()
            if int(fields[1].split(":")[1], 16) == PORT and fields[3] == "0A":
                inodes.add(f"socket:[{fields[9]}]")
    return inodes


def listeners():
    inodes = listener_inodes()
    if not inodes:
        return set()
    owners = set()
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            for fd in (proc / "fd").iterdir():
                try:
                    if os.readlink(fd) in inodes:
                        owners.add(int(proc.name))
                        break
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    continue
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue
    if not owners and inodes & listener_inodes():
        raise RuntimeError(f"Port {PORT} is occupied but its owner cannot be inspected.")
    return owners


def executable(pid):
    return os.readlink(f"/proc/{pid}/exe").removesuffix(" (deleted)")


def identity(pid):
    # Start time prevents a recycled PID from being treated as the same server.
    return Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[19]


def owned_server():
    owners = listeners()
    if not owners:
        return None
    if len(owners) != 1:
        raise RuntimeError(f"Multiple listener owners on port {PORT}: {sorted(owners)}")
    pid = owners.pop()
    if executable(pid) != str(BINARY):
        raise RuntimeError(f"Port {PORT} belongs to another program (PID {pid}); refusing to stop it.")
    return pid


def is_current(pid):
    """True when the running process executes the binary currently on disk."""
    current = os.stat(f"/proc/{pid}/exe")
    disk = BINARY.stat()
    return (current.st_dev, current.st_ino) == (disk.st_dev, disk.st_ino)


def describe(pid):
    current = os.stat(f"/proc/{pid}/exe")
    freshness = "current build" if is_current(pid) else "OLD build; run ./start.sh restart"
    built = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(current.st_mtime))
    print(f"FMLE running: PID {pid}, {freshness}, binary built {built}\n{URL}", flush=True)


def stop(pid):
    started = identity(pid)
    # Open a pidfd before signaling so a recycled PID cannot receive the signal.
    fd = os.pidfd_open(pid)
    try:
        if identity(pid) != started or executable(pid) != str(BINARY):
            raise RuntimeError("Server identity changed; retry the command.")
        signal.pidfd_send_signal(fd, signal.SIGTERM)
        for _ in range(100):
            try:
                if identity(pid) != started or pid not in listeners():
                    print(f"Stopped FMLE PID {pid}.", flush=True)
                    return
            except FileNotFoundError:
                print(f"Stopped FMLE PID {pid}.", flush=True)
                return
            time.sleep(0.1)
        raise RuntimeError(f"PID {pid} did not release port {PORT}; no replacement started.")
    finally:
        os.close(fd)


def open_browser():
    if os.environ.get("FMLEV_NO_BROWSER") == "1":
        return
    import shutil
    for command in ("xdg-open", "sensible-browser"):
        if shutil.which(command):
            subprocess.Popen([command, URL], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            return


def watch_existing(pid):
    """A second foreground launcher stays visible and can stop the existing server."""
    fd = os.pidfd_open(pid)
    try:
        if executable(pid) != str(BINARY):
            raise RuntimeError("Server identity changed; retry the command.")
        def terminate(signum, frame):
            try:
                signal.pidfd_send_signal(fd, signal.SIGTERM)
            except ProcessLookupError:
                pass
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, terminate)
        print("Attached to existing server. Ctrl+C stops it; ./start.sh restart refreshes it.", flush=True)
        select.select([fd], [], [])
    finally:
        os.close(fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("start", "restart", "stop", "status"), nargs="?", default="start")
    parser.add_argument("--background", action="store_true", help="Write server output to ~/.local/state/fmlev/server.log")
    args = parser.parse_args()
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    child = None
    with (STATE / "control.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        pid = owned_server()
        if args.action == "status":
            describe(pid) if pid else print("FMLE is stopped.")
            return
        if args.action == "stop":
            stop(pid) if pid else print("FMLE is already stopped.")
            return
        # Always build first (a no-op when nothing changed). A failed build keeps the running server.
        subprocess.run(["cargo", "build", "--release", "-p", "fmlev-cli"], cwd=ROOT, check=True)
        if args.action == "start" and pid and is_current(pid):
            describe(pid)
            print("Already running the current build; no duplicate started. ./start.sh stop terminates it.")
            open_browser()
            fcntl.flock(lock, fcntl.LOCK_UN)
            if not args.background:
                watch_existing(pid)
            return
        if pid:
            if args.action == "start":
                print(f"Running server (PID {pid}) uses an outdated binary; replacing it with the new build.", flush=True)
            stop(pid)
        log = (STATE / "server.log").open("a") if args.background else None
        child = subprocess.Popen([str(BINARY), "serve", "--port", str(PORT)], cwd=ROOT,
                                 stdout=log, stderr=log, start_new_session=True)
        if log:
            log.close()
        try:
            for _ in range(200):
                if child.poll() is not None:
                    raise RuntimeError(f"Server exited with status {child.returncode}; see terminal output or {STATE / 'server.log'}.")
                if child.pid in listeners():
                    with urllib.request.urlopen(URL, timeout=2) as response:
                        if response.status == 200:
                            break
                time.sleep(0.1)
            else:
                raise RuntimeError("Server did not become ready within 20 seconds.")
        except BaseException:
            child.terminate()
            child.wait(timeout=10)
            raise
        describe(child.pid)
        if args.background:
            print(f"Background log: {STATE / 'server.log'}\nStop: ./start.sh stop | Refresh: ./start.sh restart --background", flush=True)
        else:
            print("Server stays in this terminal. Ctrl+C stops it.\nOther terminal: ./start.sh status | ./start.sh stop | ./start.sh restart", flush=True)
        open_browser()
    if args.background:
        return
    # Keep the launcher alive; the desktop terminal remains visible until stop.
    def terminate(signum, frame):
        if child.poll() is None:
            child.terminate()
    signal.signal(signal.SIGINT, terminate)
    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGHUP, terminate)
    try:
        result = child.wait()
    finally:
        if child.poll() is None:
            child.terminate()
            child.wait(timeout=10)
    if result not in (0, -signal.SIGTERM, -signal.SIGINT):
        raise RuntimeError(f"Server exited with status {result}.")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        print(f"FMLE: {error}", file=sys.stderr)
        sys.exit(1)
