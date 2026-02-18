from __future__ import annotations

import json
import shutil
import socket
import subprocess
import time
import uuid
from pathlib import Path
from typing import Optional, Tuple

DEFAULT_CHROME_BIN = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
DEFAULT_USER_DATA_DIR = str(Path.home() / "Library/Application Support/Google/Chrome")


class ChromeSession:
    def __init__(self, proc: Optional[subprocess.Popen], cloned_root: Optional[Path], profile: str):
        self.proc = proc
        self.cloned_root = cloned_root
        self.profile = profile


def parse_cdp_host_port(cdp_url: str) -> Tuple[str, int]:
    value = cdp_url.strip()
    for prefix in ("http://", "https://"):
        if value.startswith(prefix):
            value = value[len(prefix) :]
    if "/" in value:
        value = value.split("/", 1)[0]
    if ":" in value:
        host, port_str = value.rsplit(":", 1)
        return host, int(port_str)
    return value, 80


def is_port_open(host: str, port: int, timeout: float = 0.5) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        return sock.connect_ex((host, port)) == 0


def wait_port(host: str, port: int, timeout_s: float = 12.0) -> bool:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if is_port_open(host, port):
            return True
        time.sleep(0.25)
    return False


def resolve_profile_dir(user_data_dir: Path, explicit_profile: str) -> str:
    if explicit_profile:
        return explicit_profile

    local_state = user_data_dir / "Local State"
    if local_state.exists():
        try:
            data = json.loads(local_state.read_text(encoding="utf-8"))
            last = data.get("profile", {}).get("last_used")
            if isinstance(last, str) and last:
                return last
        except Exception:
            pass

    return "Default"


def _skip_name(name: str) -> bool:
    skip = {
        "Cache",
        "Code Cache",
        "GPUCache",
        "Service Worker",
        "ShaderCache",
        "GrShaderCache",
        "Crashpad",
    }
    return name in skip or name.endswith("Cache")


def clone_profile(user_data_root: Path, profile_dir: str) -> Path:
    target_root = Path("/tmp") / f"chrome-cdp-clone-{uuid.uuid4().hex[:8]}"
    target_root.mkdir(parents=True, exist_ok=True)

    local_state = user_data_root / "Local State"
    if local_state.exists():
        shutil.copy2(local_state, target_root / "Local State")

    src = user_data_root / profile_dir
    dst = target_root / profile_dir
    if not src.exists():
        raise RuntimeError(f"Profile folder not found: {src}")

    def _ignore(_path: str, names: list[str]) -> set[str]:
        return {n for n in names if _skip_name(n)}

    shutil.copytree(src, dst, ignore=_ignore, dirs_exist_ok=True)
    return target_root


def _launch_chrome(chrome_bin: str, port: int, user_data_dir: str, profile_dir: str) -> subprocess.Popen:
    cmd = [
        chrome_bin,
        f"--remote-debugging-port={port}",
        f"--user-data-dir={user_data_dir}",
        f"--profile-directory={profile_dir}",
        "--no-first-run",
        "--no-default-browser-check",
        "--new-window",
        "about:blank",
    ]
    return subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def ensure_chrome_with_cdp(
    cdp_url: str,
    chrome_bin: str,
    user_data_dir: str,
    profile_directory: str = "",
    start_chrome: bool = False,
) -> ChromeSession:
    profile = profile_directory or "Default"

    if not start_chrome:
        return ChromeSession(proc=None, cloned_root=None, profile=profile)

    host, port = parse_cdp_host_port(cdp_url)
    if is_port_open(host, port):
        return ChromeSession(proc=None, cloned_root=None, profile=profile)

    source_root = Path(user_data_dir).expanduser()
    profile = resolve_profile_dir(source_root, profile_directory)

    # Attempt 1: direct profile
    proc = _launch_chrome(chrome_bin, port, str(source_root), profile)
    if wait_port(host, port):
        return ChromeSession(proc=proc, cloned_root=None, profile=profile)

    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except Exception:
            proc.kill()

    # Attempt 2: cloned profile (works while normal Chrome is open)
    cloned_root = clone_profile(source_root, profile)
    proc2 = _launch_chrome(chrome_bin, port, str(cloned_root), profile)
    if wait_port(host, port):
        return ChromeSession(proc=proc2, cloned_root=cloned_root, profile=profile)

    if proc2.poll() is None:
        proc2.terminate()
        try:
            proc2.wait(timeout=2)
        except Exception:
            proc2.kill()

    raise RuntimeError("Cannot open Chrome CDP port")
