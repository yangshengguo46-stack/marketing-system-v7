#!/usr/bin/env python3
"""Verify preserved source files; no network, mutation, or product execution."""
from pathlib import Path
import hashlib
import json
import sys

def main() -> int:
    here = Path(__file__).resolve().parent
    repo_root = here.parents[2]
    manifest = json.loads((here / "SYNC_MANIFEST.json").read_text(encoding="utf-8"))
    errors = []
    for item in manifest["files"]:
        path = (repo_root / item["path"]).resolve()
        if not path.is_relative_to(here) or not path.is_file():
            errors.append(f"Missing or unsafe: {item['path']}")
            continue
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        if len(data) != item["bytes"] or digest != item["sha256"] or blob != item["git_blob_sha1"]:
            errors.append(f"Content mismatch: {item['path']}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"Verified {len(manifest['files'])} preserved source files. No product tests were run.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
