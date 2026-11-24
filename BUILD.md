# Build OpenBoatUtils for Nova

Use a temporary checkout of kipd0/nova-ios at commit 9a73eceb0ef99bf4de15ad4d431c373371112767 from the newupdate branch. Extract the openboatutils directory from this archive directly inside the checkout so its Cargo.toml can resolve ../crates/horizon-mod-api.

From the checkout root with rustup available, run the following commands.

```sh
rustup toolchain install 1.99.1 --profile minimal --component rust-src --target wasm32-unknown-unknown
cargo +1.99.1 install --path crates/cargo-horizon-mod
cd openboatutils
cargo +1.99.1 test --lib
cargo +1.99.1 horizon-mod build --wasm
cargo +1.99.1 horizon-mod check target/horizon-mod/openboatutils.hmod
```

The output package is target/horizon-mod/openboatutils.hmod. The build tool generates the wasm manifest fields.
