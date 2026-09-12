"""Generate LICENSES: this project's own license plus every dependency's,
via cargo-about.

Usage:
    python scripts/licenses.py
"""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def fix_name() -> str:
    """This crate's package name, read from Cargo.toml so it cannot drift."""
    manifest = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'^name\s*=\s*"([^"]+)"', manifest, re.M)
    if not match:
        sys.exit("Error: could not read the package name from Cargo.toml")
    return match.group(1)


def licenses() -> None:
    about_hbs = ROOT / "about.hbs"
    if not about_hbs.is_file():
        sys.exit(f"Error: {about_hbs} not found.")

    print("Generating third-party license text (cargo about)...")
    # cargo-about refuses to write straight to a pipe on Windows (it can come
    # out re-encoded, e.g. license text going through as UTF-16), so it has to
    # write to a real file via -o rather than being captured from stdout.
    with tempfile.TemporaryDirectory() as tmp:
        third_party_path = Path(tmp) / "third-party.txt"
        result = subprocess.run(
            ["cargo", "about", "generate", about_hbs.name, "-o", str(third_party_path)],
            cwd=ROOT, capture_output=True, text=True,
        )
        if result.returncode != 0:
            sys.exit(
                "Error: `cargo about generate` failed -- is cargo-about installed?\n"
                'Install with: cargo install cargo-about --features="cli"\n\n'
                f"{result.stderr}"
            )
        third_party_text = third_party_path.read_text(encoding="utf-8")

    own_license = (ROOT / "LICENSE").read_text(encoding="utf-8").strip()
    banner = "=" * 80

    dest = ROOT / "LICENSES"
    dest.write_text(f"{banner}\n{fix_name()}\n{banner}\n{own_license}\n\n{third_party_text}", encoding="utf-8")
    print(f"Wrote -> {dest}")


if __name__ == "__main__":
    licenses()
