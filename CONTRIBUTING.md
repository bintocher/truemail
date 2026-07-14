**English** · [Русский](CONTRIBUTING.ru.md)

# Contributing to truemail

1. Discuss substantial changes in an issue before implementation.
2. Do not commit tokens, passwords, `.env` files or local databases.
3. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   and `cargo test --workspace` before opening a pull request.
4. Add tests for protocol, storage and security behavior changed by the patch.
5. Include `I have read and agree to CLA.md` in the pull request description.

Bug reports must remove personal mail contents, OAuth tokens and local paths
before attaching logs.
