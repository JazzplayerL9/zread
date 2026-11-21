#!/usr/bin/env bash
set -euo pipefail

HOST="${1:-pi@192.168.1.192}"

cargo clippy --all-targets --offline
cargo test --offline
cargo build --release --target aarch64-unknown-linux-musl --offline

scp target/aarch64-unknown-linux-musl/release/zread "${HOST}:/home/pi/zread.bin"
ssh "${HOST}" 'sudo install -m 755 /home/pi/zread.bin /usr/local/bin/zread && rm -f /home/pi/zread.bin'
echo "Deployed zread to ${HOST}"
