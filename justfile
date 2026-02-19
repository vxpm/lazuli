mod ipl-hle "crates/ipl-hle"
mod dspint "crates/dspint"

export RUSTDOCFLAGS := "-Zunstable-options --show-type-layout --generate-link-to-definition --default-theme dark"

# Lists all recipes
list:
    @just --list

# Opens the documentation of the crates
doc:
    cargo doc --open
