# Contributing

Thanks for contributing to `uentry`.

This project is published under AGPL-3.0-or-later and may also be distributed under commercial terms.
To keep that dual-licensing path available, all contributors must accept the CLA.

## Before opening a PR

1. Read [CLA.md](CLA.md).
1. In your first PR description, add this exact sentence:

`I have read and agree to the CLA in CLA.md.`

1. Ensure your contribution is your original work or you have rights to submit it.
1. Sign off each commit with Developer Certificate of Origin (DCO):

`git commit -s -m "your message"`

For existing commits:

`git rebase --signoff <base-branch>`

## Development checks

Run the standard local checks before submitting:

- `cargo fmt`
- `cargo clippy -- -D warnings`
- `cargo test`

Container scenarios also exist and require Docker:

- `cargo test --test container_tests`

## Troubleshooting legal checks

If a PR fails legal validation:

- **CLA check failed**: add this exact sentence to the PR description:

`I have read and agree to the CLA in CLA.md.`

- **DCO check failed**: add sign-offs to commits.

Single commit:

`git commit --amend -s`

Multiple commits:

`git rebase --signoff <base-branch>`

After either fix, push updates to the PR branch (use `--force-with-lease` if history changed).

## Pull request expectations

- Keep changes focused and minimal.
- Include tests or documentation updates when behavior changes.
- Do not include secrets or credentials.
- Confirm the PR checklist items (including CLA acceptance).
