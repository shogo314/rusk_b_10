#!/usr/bin/env python3
"""Check or update formatting of the standard library and all examples."""
import argparse
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='update files instead of checking')
    args = parser.parse_args()
    binary = ROOT / 'target' / 'debug' / 'rusk'
    if not binary.is_file():
        print('Build the formatter first: cargo build --locked', file=sys.stderr)
        return 1
    sources = sorted(path for folder in ('lib', 'examples') for path in (ROOT / folder).rglob('*.rusk'))
    if not sources:
        print('No Rusk library or example sources found', file=sys.stderr)
        return 1
    mode = '--write' if args.write else '--check'
    completed = subprocess.run([str(binary), 'fmt', mode, *map(str, sources)], cwd=ROOT, check=False)
    if completed.returncode == 0:
        print(f'{"Processed" if args.write else "Checked"} {len(sources)} Rusk files')
    return completed.returncode


if __name__ == '__main__':
    sys.exit(main())
