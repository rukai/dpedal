#!/usr/bin/env bash
# Apply formatting to all projects
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo fmt --all
cd dpedal_config_web_app
cargo fmt --all
cd ../dpedal_firmware
cargo fmt --all