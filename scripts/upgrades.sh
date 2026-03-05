#!/usr/bin/env bash
# depdencies:
# cargo install -f cargo-upgrades --version 2.1.1
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo upgrades
cd dpedal_config_web_app
cargo upgrades
cd ../dpedal_firmware
cargo upgrades
