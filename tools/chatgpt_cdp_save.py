#!/usr/bin/env python3
"""
Connect to a local Chrome via CDP, open ChatGPT with existing login session,
send one question, and save the latest assistant reply to a file.
"""

from __future__ import annotations

import argparse
import json
import shutil
import socket
import subprocess
import sys
import time
import uuid
from datetime import datetime
from pathlib import Path
from typing import Optional, Tuple

from playwright.sync_api import sync_playwright


DEFAULT_CDP = "http://127.0.0.1:9222"
DEFAULT_CHATGPT_URL = "https://chatgpt.com/"
DEFAULT_CHROME_BIN = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
DEFAULT_USER_DATA_DIR = str(Path.home() / "Library/Application Support/Google/Chrome")


def is_port_open(host: str, port: int, timeout: float = 0.5) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        return sock.connect_ex((host, port)) == 0


def parse_cdp_host_port(cdp_url: str) -> Tuple[str, int]:
    value = cdp_url.strip()
    if value.startswith("http://"):
        value = value[len("http://") :]
    elif value.startswith("https://"):
        value = value[len("https://") :]
    if "/" in value:
        value = value.split("/", 1)[0]

    if ":" in value:
        host, port_str = value.rsplit(":", 1)
        return host, int(port_str)
    return value, 80


def resolve_profile_dir(user_data_dir: Path, explicit: str) -> str:
    if explicit:
        return explicit

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


def should_skip_dir(name: str) -> bool:
    skip_exact = {
        "Cache",
        "Code Cache",
        "GPUCache",
        "Service Worker",
        "ShaderCache",
        "GrShaderCache",
        "Crashpad",
        "BrowserMetrics",
        "Safe Browsing",
        "optimization_guide_model_store",
        "Feature Engagement Tracker",
    }
    return name in skip_exact or name.endswith("Cache")


def clone_profile_tree(source_root: Path, source_profile: str) -> Path:
    target_root = Path("/tmp") / f"chrome-cdp-clone-{uuid.uuid4().hex[:8]}"
    target_root.mkdir(parents=True, exist_ok=True)

    local_state = source_root / "Local State"
    if local_state.exists():
        shutil.copy2(local_state, target_root / "Local State")

    src_profile_dir = source_root / source_profile
    dst_profile_dir = target_root / source_profile
    if not src_profile_dir.exists():
        raise RuntimeError(
            f"Profile folder not found: {src_profile_dir}. "
            "Use --profile-directory to specify the right one."
        )

    def _ignore(path: str, names: list[str]) -> set[str]:
        ignored = set()
        for n in names:
            if should_skip_dir(n):
                ignored.add(n)
        return ignored

    shutil.copytree(src_profile_dir, dst_profile_dir, ignore=_ignore, dirs_exist_ok=True)
    return target_root


def build_chrome_cmd(chrome_bin: str, port: int, user_data_dir: str, profile_directory: str) -> list[str]:
    return [
        chrome_bin,
        f"--remote-debugging-port={port}",
        f"--user-data-dir={user_data_dir}",
        f"--profile-directory={profile_directory}",
        "--no-first-run",
        "--no-default-browser-check",
        "--new-window",
        "about:blank",
    ]


def wait_cdp_ready(host: str, port: int, max_wait_s: float = 12.0) -> bool:
    deadline = time.time() + max_wait_s
    while time.time() < deadline:
        if is_port_open(host, port):
            return True
        time.sleep(0.25)
    return False


def maybe_start_chrome(args: argparse.Namespace) -> tuple[Optional[subprocess.Popen], Optional[Path], str]:
    if not args.start_chrome:
        return None, None, args.profile_directory or "Default"

    host, port = parse_cdp_host_port(args.cdp_url)
    if is_port_open(host, port):
        print(f"[info] CDP port {host}:{port} already open, skip launching Chrome")
        return None, None, args.profile_directory or "Default"

    source_root = Path(args.user_data_dir).expanduser()
    profile_dir = resolve_profile_dir(source_root, args.profile_directory or "")

    # First try direct launch. If Chrome is already running with same profile, this can fail.
    print(f"[info] Launching Chrome with CDP (profile={profile_dir})...")
    cmd = build_chrome_cmd(args.chrome_bin, port, str(source_root), profile_dir)
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    if wait_cdp_ready(host, port):
        print(f"[info] CDP port is ready at {host}:{port}")
        return proc, None, profile_dir

    # Fallback: clone profile into temp directory and launch isolated instance.
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except Exception:
            proc.kill()

    print("[warn] Direct launch failed, cloning profile to temp dir and retrying...")
    cloned_root = clone_profile_tree(source_root, profile_dir)
    cloned_cmd = build_chrome_cmd(args.chrome_bin, port, str(cloned_root), profile_dir)
    proc2 = subprocess.Popen(cloned_cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    if wait_cdp_ready(host, port):
        print(f"[info] CDP port is ready at {host}:{port} (using cloned profile)")
        return proc2, cloned_root, profile_dir

    if proc2.poll() is None:
        proc2.terminate()
        try:
            proc2.wait(timeout=2)
        except Exception:
            proc2.kill()

    raise RuntimeError(
        "Chrome started but CDP port did not open, even with cloned profile. "
        "Check chrome path and local security policy."
    )


def locator_first(page, selectors: list[str]):
    for sel in selectors:
        loc = page.locator(sel)
        if loc.count() > 0:
            return loc.first
    return None


def get_assistant_locator(page):
    selectors = [
        '[data-message-author-role="assistant"]',
        "article [data-message-author-role='assistant']",
        "[data-testid*='assistant']",
    ]
    for sel in selectors:
        loc = page.locator(sel)
        try:
            if loc.count() > 0:
                return loc
        except Exception:
            continue
    return page.locator(selectors[0])


def assistant_count(page) -> int:
    loc = get_assistant_locator(page)
    try:
        return loc.count()
    except Exception:
        return 0


def send_question(page, question: str) -> None:
    input_selectors = [
        "#prompt-textarea",
        "textarea[placeholder*='Message']",
        "textarea",
        "div[role='textbox'][contenteditable='true']",
    ]

    box = None
    for _ in range(20):
        box = locator_first(page, input_selectors)
        if box is not None:
            break
        time.sleep(0.5)

    if box is None:
        raise RuntimeError(
            "Cannot find ChatGPT input box. Are you logged in at https://chatgpt.com/?"
        )

    box.click()
    try:
        box.fill(question)
    except Exception:
        page.keyboard.type(question)

    # Prefer Enter submit first.
    submitted = False
    try:
        box.press("Enter")
        submitted = True
    except Exception:
        submitted = False

    if submitted:
        return

    send_btn = locator_first(
        page,
        [
            "button[data-testid='send-button']",
            "button[aria-label*='Send']",
            "button:has-text('Send')",
        ],
    )
    if send_btn is not None:
        send_btn.click()
        return

    raise RuntimeError("Failed to submit question: no Enter or send button available")


def extract_latest_answer(page) -> str:
    banned = (
        "chatgpt can make mistakes",
        "thinking",
        "reasoned for",
        "searching the web",
    )

    def clean_text(raw: str) -> str:
        lines = [x.strip() for x in raw.splitlines() if x.strip()]
        kept = []
        for line in lines:
            lower = line.lower()
            if any(k in lower for k in banned):
                continue
            kept.append(line)
        return "\n".join(kept).strip()

    loc = get_assistant_locator(page)
    try:
        count = loc.count()
    except Exception:
        return ""
    if count <= 0:
        return ""

    start = max(0, count - 6)
    for i in range(count - 1, start - 1, -1):
        node = loc.nth(i)
        for inner_sel in [".markdown", "[data-message-content='true']"]:
            try:
                raw = node.locator(inner_sel).first.inner_text(timeout=1000).strip()
                text = clean_text(raw)
                if len(text) > 1:
                    return text
            except Exception:
                pass
        try:
            raw = node.inner_text(timeout=1200).strip()
            text = clean_text(raw)
            if len(text) > 1:
                return text
        except Exception:
            pass
    return ""


def wait_answer_stable(page, before_assistant_count: int, timeout_s: int = 180) -> str:
    deadline = time.time() + timeout_s
    last = ""
    stable_rounds = 0

    while time.time() < deadline:
        page.wait_for_timeout(2000)
        if assistant_count(page) <= before_assistant_count:
            continue
        text = extract_latest_answer(page)

        if text and text == last:
            stable_rounds += 1
        else:
            stable_rounds = 0
            last = text

        # 3 consecutive stable polls ~= 6s unchanged
        if len(last) > 1 and stable_rounds >= 2:
            return last

    if last:
        return last
    raise TimeoutError("Timed out waiting for assistant reply")


def normalize_answer_text(answer: str, question: str) -> str:
    text = answer.replace("\r\n", "\n").strip()
    lower = text.lower()

    marker = "chatgpt said:"
    if marker in lower:
        pos = lower.find(marker)
        text = text[pos + len(marker) :].strip()
        lower = text.lower()

    if text.startswith(question):
        text = text[len(question) :].strip()
        lower = text.lower()

    if lower.startswith("you said:"):
        lines = [x for x in text.splitlines() if x.strip()]
        if len(lines) >= 3:
            text = "\n".join(lines[2:]).strip()

    return text if text else answer.strip()


def save_output(path: Path, question: str, answer: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.suffix.lower() == ".json":
        payload = {
            "timestamp": datetime.now().isoformat(timespec="seconds"),
            "question": question,
            "answer": answer,
        }
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    else:
        body = (
            f"# ChatGPT Capture\n\n"
            f"- time: {datetime.now().isoformat(timespec='seconds')}\n\n"
            f"## Question\n{question}\n\n"
            f"## Answer\n{answer}\n"
        )
        path.write_text(body, encoding="utf-8")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Send one question to ChatGPT in local Chrome session and save reply"
    )
    parser.add_argument("--question", required=True, help="Question text to send")
    parser.add_argument(
        "--output",
        default=str(Path.cwd() / f"chatgpt-answer-{datetime.now().strftime('%Y%m%d-%H%M%S')}.md"),
        help="Output file (.md or .json)",
    )
    parser.add_argument("--cdp-url", default=DEFAULT_CDP, help="CDP endpoint URL")
    parser.add_argument("--chatgpt-url", default=DEFAULT_CHATGPT_URL, help="ChatGPT URL")

    parser.add_argument(
        "--start-chrome",
        action="store_true",
        help="Start Chrome with CDP if port is not open",
    )
    parser.add_argument("--chrome-bin", default=DEFAULT_CHROME_BIN, help="Chrome binary path")
    parser.add_argument(
        "--user-data-dir",
        default=DEFAULT_USER_DATA_DIR,
        help="Chrome user data dir (for reusing login state)",
    )
    parser.add_argument(
        "--profile-directory",
        default="",
        help="Chrome profile folder name, e.g. Default or 'Profile 1' (auto-detect when empty)",
    )
    parser.add_argument(
        "--close-tab",
        action="store_true",
        help="Close the newly opened ChatGPT tab after saving answer",
    )
    parser.add_argument(
        "--cleanup-clone",
        action="store_true",
        help="Remove temporary cloned profile dir after run",
    )

    return parser


def main() -> int:
    args = build_parser().parse_args()
    output = Path(args.output).expanduser().resolve()

    chrome_proc = None
    cloned_root: Optional[Path] = None
    selected_profile = args.profile_directory or "Default"

    chrome_proc, cloned_root, selected_profile = maybe_start_chrome(args)
    if args.start_chrome:
        print(f"[info] Using Chrome profile: {selected_profile}")
        if cloned_root:
            print(f"[info] Cloned user data dir: {cloned_root}")

    try:
        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp(args.cdp_url)
            context = browser.contexts[0] if browser.contexts else browser.new_context()
            page = context.new_page()

            page.goto(args.chatgpt_url, wait_until="domcontentloaded", timeout=120000)
            page.wait_for_timeout(3000)

            if "login" in page.url.lower() or "auth" in page.url.lower():
                raise RuntimeError(
                    "Looks not logged in. Open ChatGPT in this Chrome profile and login once, then retry."
                )

            before = assistant_count(page)
            send_question(page, args.question)
            answer = wait_answer_stable(page, before_assistant_count=before)
            answer = normalize_answer_text(answer, args.question)
            save_output(output, args.question, answer)

            print(f"[ok] Saved answer to: {output}")
            print(f"[ok] Answer preview: {answer[:160].replace(chr(10), ' ')}")

            if args.close_tab:
                page.close()
    finally:
        if chrome_proc is not None and chrome_proc.poll() is None:
            # Keep Chrome alive by default (same as user session expectation)
            pass
        if cloned_root and cloned_root.exists() and args.cleanup_clone:
            shutil.rmtree(cloned_root, ignore_errors=True)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[error] {exc}", file=sys.stderr)
        raise SystemExit(1)
