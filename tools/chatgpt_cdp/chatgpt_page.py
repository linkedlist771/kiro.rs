from __future__ import annotations

import time


def _first_locator(page, selectors: list[str]):
    for sel in selectors:
        loc = page.locator(sel)
        try:
            if loc.count() > 0:
                return loc.first
        except Exception:
            continue
    return None


def assistant_count(page) -> int:
    loc = page.locator('[data-message-author-role="assistant"]')
    try:
        return loc.count()
    except Exception:
        return 0


def open_chatgpt(page, url: str) -> None:
    page.goto(url, wait_until="domcontentloaded", timeout=120000)
    page.wait_for_timeout(3000)
    lower = page.url.lower()
    if "login" in lower or "auth" in lower:
        raise RuntimeError("ChatGPT not logged in for this Chrome profile")


def send_question(page, question: str) -> None:
    box = None
    for _ in range(20):
        box = _first_locator(
            page,
            [
                "#prompt-textarea",
                "textarea[placeholder*='Message']",
                "textarea",
                "div[role='textbox'][contenteditable='true']",
            ],
        )
        if box is not None:
            break
        time.sleep(0.5)

    if box is None:
        raise RuntimeError("Cannot find ChatGPT input box")

    box.click()
    try:
        box.fill(question)
    except Exception:
        page.keyboard.type(question)

    try:
        box.press("Enter")
        return
    except Exception:
        pass

    send_btn = _first_locator(
        page,
        [
            "button[data-testid='send-button']",
            "button[aria-label*='Send']",
            "button:has-text('Send')",
        ],
    )
    if send_btn is None:
        raise RuntimeError("Cannot submit question")
    send_btn.click()


def latest_assistant_text(page) -> str:
    assistants = page.locator('[data-message-author-role="assistant"]')
    try:
        count = assistants.count()
    except Exception:
        return ""
    if count <= 0:
        return ""

    node = assistants.nth(count - 1)
    for inner_sel in [".markdown", "[data-message-content='true']"]:
        try:
            txt = node.locator(inner_sel).first.inner_text(timeout=1500).strip()
            if txt:
                return txt
        except Exception:
            pass

    try:
        return node.inner_text(timeout=1500).strip()
    except Exception:
        return ""


def wait_new_answer(page, before_assistant_count: int, timeout_s: int = 180) -> str:
    deadline = time.time() + timeout_s
    last = ""
    stable = 0

    while time.time() < deadline:
        page.wait_for_timeout(1500)
        if assistant_count(page) <= before_assistant_count:
            continue

        txt = latest_assistant_text(page)
        if not txt:
            continue

        if txt == last:
            stable += 1
        else:
            last = txt
            stable = 0

        if stable >= 2:
            return last

    if last:
        return last
    raise TimeoutError("Timed out waiting for assistant answer")
