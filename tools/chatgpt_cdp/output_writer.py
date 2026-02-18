from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path


def save_capture(path: Path, question: str, answer: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)

    if path.suffix.lower() == ".json":
        payload = {
            "timestamp": datetime.now().isoformat(timespec="seconds"),
            "question": question,
            "answer": answer,
        }
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
        return

    text = (
        "# ChatGPT Capture\n\n"
        f"- time: {datetime.now().isoformat(timespec='seconds')}\n\n"
        f"## Question\n{question}\n\n"
        f"## Answer\n{answer}\n"
    )
    path.write_text(text, encoding="utf-8")
