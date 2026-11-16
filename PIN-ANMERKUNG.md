# Pin note (not yet existing tags)

`crates/fs-rpc/Cargo.toml` pins by tag: `fs-interfaces` **v0.0.26** and `fs-objects` **v0.0.4**. Neither tag exists on GitHub yet (checked with `git ls-remote`, 2026-11-16); they are to be set on these heads (branches `unreviewed/...`):

| Tag | Repo | Head the lock was made against |
|---|---|---|
| v0.0.26 | fs-interfaces | 9df19ee323145119e89c003e64982be68e9fc1da (`unreviewed/interfaces-026-ohne-rpc`) |
| v0.0.4 | fs-objects | e184999a1bf011c98040594ddb23ea466ffb5663 (`unreviewed/objects-004-pin`) |

`Cargo.lock` was generated and all gates were run with **local mirrors** that carry exactly these two tags (`git url.<mirror>.insteadOf` and `CARGO_NET_GIT_FETCH_WITH_CLI=true`, no `[patch]`, no path override). The manifest and the lock are therefore the final form: once the tags are set on exactly these commits, `cargo test --locked` works unchanged. If a head changes before tagging, regenerate the lock (`cargo update -p fs-interfaces -p fs-objects`) and re-run the gates.

Until then this repository does NOT build against GitHub. Not verified: the build against the real tags (they do not exist).
