"""Download and install Ultimate ASI Loader into a game's shipping executable folder.

Usage:
    python scripts/loader.py <path to FFRS/Binaries/Win64 folder>
"""
import argparse
import hashlib
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

LOADER_URL = "https://github.com/ThirteenAG/Ultimate-ASI-Loader/releases/download/x64-latest/winmm-x64.zip"
LOADER_DLL_NAME = "winmm.dll"
LOADER_HASH_NAME = "winmm-x64.SHA512"


def install_loader(game_folder: str) -> None:
    game_folder = Path(game_folder)
    if not game_folder.is_dir():
        sys.exit(f"Error: '{game_folder}' does not exist or is not a directory.")

    print(f"Downloading Ultimate ASI Loader from {LOADER_URL}...")
    with urllib.request.urlopen(LOADER_URL) as response:
        archive = zipfile.ZipFile(io.BytesIO(response.read()))

    dll_bytes = archive.read(LOADER_DLL_NAME)
    actual_hash = hashlib.sha512(dll_bytes).hexdigest()

    expected_hash = None
    if LOADER_HASH_NAME in archive.namelist():
        for line in archive.read(LOADER_HASH_NAME).decode().splitlines():
            if line.strip().lower().startswith("hash"):
                expected_hash = line.split(":", 1)[1].strip().lower()
                break

    if expected_hash is None:
        sys.exit(f"Error: no checksum found in archive for {LOADER_DLL_NAME} -- refusing to install unverified")
    if actual_hash != expected_hash:
        sys.exit(
            f"Error: {LOADER_DLL_NAME} checksum mismatch "
            f"(expected {expected_hash}, got {actual_hash}) -- refusing to install"
        )
    print(f"Checksum verified ({actual_hash[:16]}...)")

    dest = game_folder / LOADER_DLL_NAME
    dest.write_bytes(dll_bytes)
    print(f"Installed loader -> {dest}")
    print("Done!")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game_folder", help="path to the 'FINAL FANTASY RESONANCE' folder")
    args = parser.parse_args()
    install_loader(args.game_folder)


if __name__ == "__main__":
    main()
