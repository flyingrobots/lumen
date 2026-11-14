# Lumen

A deterministic ray tracer: a [Geordi](https://github.com/flyingrobots/geordi) runtime built on
[Bunny](https://github.com/flyingrobots/bunny)'s fixed-point math. The same scene renders to the same bytes on
every machine.

Nothing is implemented yet. Work is tracked in GitHub Issues with `issue-dag/2` frontmatter.

## License

Apache-2.0. See [LICENSE](LICENSE).

## Build and test

Builds and tests run in the shared containers through `devrun`, never on the host
(see `~/git/devcontainers/README.md`). The toolchain is pinned to 1.96.0 in
`rust-toolchain.toml`, the version of the container image.

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch -- cargo fmt --all --check
python3 $D lumen ~/git/lumen --worktree -- cargo clippy --workspace --all-targets --locked -- -D warnings
python3 $D lumen ~/git/lumen --worktree -- cargo test --locked --workspace
python3 $D lumen ~/git/lumen --worktree -- cargo run --locked -q -p lumen
```

`--fetch` is needed on the first run in a worktree. Use `--commit HEAD` instead of
`--worktree` for evidence pinned to a commit.

Lint posture mirrors Bunny: `unsafe` is forbidden, clippy `all` is denied and
`pedantic` warns, and CI runs with warnings denied.

Lumen does not depend on Bunny yet. The pinned Bunny git dependency arrives with the
first rendering task, once the APIs Lumen needs are merged.
