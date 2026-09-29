# git-bull

A fast, open-source Git client for Linux, Windows and macOS, written in Rust.
Its layout follows SourceTree: repository tabs, a sidebar with branches, tags,
remotes and stashes, a commit list with graph, and commit details with diffs
below. It is built to stay fluid on repositories with more than a million
commits.

git-bull is in early development. The first milestone is a read-only viewer;
its plan lives in [`openspec/changes/add-repository-viewer`](openspec/changes/add-repository-viewer),
and the architecture decisions in [`docs/adr`](docs/adr).

## Building from source

### Prerequisites

- **Rust**, installed through [rustup](https://rustup.rs). The exact version
  is pinned in `rust-toolchain.toml`; rustup installs it on the first build.
- **A C linker for your platform:**
  - Windows: Visual Studio 2022 or its Build Tools with the workload
    "Desktop development with C++".
  - macOS: the Xcode Command Line Tools (`xcode-select --install`).
  - Linux: a C compiler such as `gcc`, for example from `build-essential` on
    Debian and Ubuntu.
- **Git 2.34 or newer** to run git-bull.

### Build and run

```sh
git clone https://github.com/BlackmillSolutions/git-bull.git
cd git-bull
cargo build --release
cargo run --release --bin git-bull
```

The binary is written to `target/release/git-bull` (`git-bull.exe` on
Windows).

### Checks

These are the checks that CI runs on every push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
```

`cargo deny` checks the licences of all dependencies and known security
advisories. Install it once with `cargo install cargo-deny --locked`.

## Licence

git-bull is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT licence ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in git-bull by you, as defined in the Apache-2.0
licence, shall be dual licensed as above, without any additional terms or
conditions.
