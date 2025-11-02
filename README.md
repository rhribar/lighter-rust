
Run: RUST_LOG=debug cargo run
Lint: /rustfmt-nightly.sh src/utils.rs -> set chmod +x rustfmt-nightly.sh on .sh

RUST_LOG=debug BOT_MODE=production cargo run