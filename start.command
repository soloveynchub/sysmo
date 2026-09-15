#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$ROOT"
./scripts/setup.sh
python3 scripts/manage.py install
python3 scripts/manage.py open
