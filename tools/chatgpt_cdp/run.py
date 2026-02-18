#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import shutil
import sys
from datetime import datetime
from pathlib import Path

from playwright.sync_api import sync_playwright

from chrome_cdp import DEFAULT_CHROME_BIN, DEFAULT_USER_DATA_DIR, ensure_chrome_with_cdp
from chatgpt_page import assistant_count, open_chatgpt, send_question, wait_new_answer
from output_writer import save_capture

DEFAULT_CDP = "http://127.0.0.1:9222"
DEFAULT_CHATGPT_URL = "https://chatgpt.com/"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Ask ChatGPT in local Chrome profile and save answer")
    parser.add_argument("--question", required=True, help="Question text")
    parser.add_argument(
        "--output",
        default=str(Path.cwd() / "tools/chatgpt_cdp/output" / f"answer-{datetime.now().strftime('%Y%m%d-%H%M%S')}.md"),
        help="Output path (.md or .json)",
    )
    parser.add_argument("--chatgpt-url", default=DEFAULT_CHATGPT_URL)
    parser.add_argument("--cdp-url", default=DEFAULT_CDP)
    parser.add_argument("--start-chrome", action="store_true", help="Auto start Chrome CDP if needed")
    parser.add_argument("--chrome-bin", default=DEFAULT_CHROME_BIN)
    parser.add_argument("--user-data-dir", default=DEFAULT_USER_DATA_DIR)
    parser.add_argument("--profile-directory", default="", help="e.g. Default, Profile 1")
    parser.add_argument("--close-tab", action="store_true")
    parser.add_argument("--cleanup-clone", action="store_true")
    parser.add_argument("--keep-chrome", action="store_true", help="Keep launched Chrome running")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output = Path(args.output).expanduser().resolve()

    session = ensure_chrome_with_cdp(
        cdp_url=args.cdp_url,
        chrome_bin=args.chrome_bin,
        user_data_dir=args.user_data_dir,
        profile_directory=args.profile_directory,
        start_chrome=args.start_chrome,
    )

    if args.start_chrome:
        print(f"[info] profile={session.profile}")
        if session.cloned_root:
            print(f"[info] cloned_profile_dir={session.cloned_root}")

    try:
        with sync_playwright() as p:
            browser = p.chromium.connect_over_cdp(args.cdp_url)
            context = browser.contexts[0] if browser.contexts else browser.new_context()
            page = context.new_page()

            open_chatgpt(page, args.chatgpt_url)
            before = assistant_count(page)
            send_question(page, args.question)
            answer = wait_new_answer(page, before_assistant_count=before)

            save_capture(output, args.question, answer)
            print(f"[ok] saved={output}")
            print(f"[ok] preview={answer[:120].replace(chr(10), ' ')}")

            if args.close_tab:
                page.close()
    finally:
        if not args.keep_chrome and session.proc is not None and session.proc.poll() is None:
            session.proc.terminate()
            try:
                session.proc.wait(timeout=3)
            except Exception:
                session.proc.kill()
        if not args.keep_chrome and session.cloned_root:
            # Chrome may respawn child processes; kill by unique cloned user-data-dir.
            subprocess.run(
                ["/usr/bin/pkill", "-f", f"--user-data-dir={session.cloned_root}"],
                check=False,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        if args.cleanup_clone and session.cloned_root and session.cloned_root.exists():
            shutil.rmtree(session.cloned_root, ignore_errors=True)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"[error] {exc}", file=sys.stderr)
        raise SystemExit(1)
