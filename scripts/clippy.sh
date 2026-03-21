#!/usr/bin/env bash
# some things to explicitly point out:
# * clippy also reports rustc warnings and errors
# * clippy --all-targets causes clippy to run against tests and examples which it doesnt do by default.
set -e

cd "$(dirname "${BASH_SOURCE[0]}")/.."

cd dpedal_config_web_app
cargo hack --feature-powerset clippy --all-targets --locked -- -D warnings
cd ../dpedal_firmware
cargo hack --feature-powerset clippy --locked -- -D warnings
cd ..
cargo hack --feature-powerset clippy --all-targets --locked -- -D warnings
