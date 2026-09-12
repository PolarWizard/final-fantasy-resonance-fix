"""Build and install the fix into the designated game directory."""
import argparse
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
NAME = "final_fantasy_resonance_fix"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("game_folder", type=Path, help="Steam game root or FFRS/Binaries/Win64")
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()

    folder = args.game_folder
    if not (folder / "FFRS-Win64-Shipping.exe").is_file():
        folder = folder / "FFRS" / "Binaries" / "Win64"

    exe = folder / "FFRS-Win64-Shipping.exe"
    if not exe.is_file():
        parser.error("FFRS-Win64-Shipping.exe not found")

    folder = folder / "scripts"
    folder.mkdir(parents=True, exist_ok=True)

    command = ["cargo", "build", "--locked"]
    if not args.debug:
        command.append("--release")
    subprocess.run(command, cwd=ROOT, check=True)
    built = ROOT / "target" / ("debug" if args.debug else "release") / (NAME + ".dll")
    shutil.copy2(built, folder / (NAME + ".asi"))
    config = folder / (NAME + ".toml")
    if not config.exists():
        shutil.copy2(ROOT / config.name, config)
    print("Installed:", folder / (NAME + ".asi"))
    print("Requires an x64 ASI loader next to the shipping executable. Restart the game.")


if __name__ == "__main__":
    main()
