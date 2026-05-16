# Contributing to rkt

Thank you for contributing. This document covers the conventions and requirements
for getting a contribution merged.

## Submitting Pull Requests

Before opening a PR:

- Read [Commit Guidelines], [Code Style], and [Testing].
- For open issues: comment with your proposed solution before writing code. This
  lets maintainers flag concerns early and point you toward efficient
  implementation paths.
- For new features: open a [feature request] first. Do not submit a PR
  implementing a feature that hasn't been discussed and accepted. We take new
  features seriously because they directly affect usability and maintenance
  burden.
- For doc fixes, typos, and wording improvements: just submit the PR.
- Purely cosmetic changes (whitespace, formatting) are not accepted unless
  they accompany a functional change.

All contributed code must be:

- **Correct** — it does what it claims, handles edge cases, and has tests.
- **Simple** — accomplishes the task as idiomatically as possible.
- **Documented** — public items have doc comments with examples.
- **Formatted** — run `cargo fmt` before committing.
- **Focused** — does what it's supposed to and nothing more.

## Commit Guidelines
[Commit Guidelines]: #commit-guidelines

Write commit messages in the imperative mood with a short subject line
(under 72 characters), e.g. `Add TLS configuration option`.

For bug fixes, name integration test files after the issue:
`short-description-NNNN.rs`, where `NNNN` is the GitHub issue number.

### AI-Assisted Contributions

If any part of a commit was written or materially shaped by an AI tool, include
an `AI-Tool:` trailer naming the tool:

```
Add rate limiting to request guard

AI-Tool: Claude Code
```

This applies to code, tests, and documentation alike. It does not disqualify a
contribution — it just keeps the project's history honest.

## Testing
[Testing]: #testing

Run `./scripts/test.sh` before submitting. The default mode covers most cases:

```sh
./scripts/test.sh              # default tests
./scripts/test.sh --examples   # user-facing API changes or dependency updates
./scripts/test.sh --core       # feature flag changes
./scripts/test.sh --ui         # codegen changes
./scripts/test.sh --help       # full option list
```

For any change that affects behavior, include a test:

- **Bug fixes** — add or modify an integration test in `tests/` named
  `short-description-rkt-NNNN.rs`.
- **New features** — unit tests, doctests, and integration tests as appropriate.
  All new public APIs must be fully documented with doctests.
- **Improved features** — modifying an existing example is a good place to add
  coverage; update the example's README if you do.

### Codegen (UI Tests)

Changes to `_codegen` crates require updating UI tests, which capture and
compare compiler output. Tests live under `codegen/tests/ui-fail` with symlinks
in sibling `ui-fail-stable` and `ui-fail-nightly` directories.

Run: `./scripts/test.sh +stable --ui` and `./scripts/test.sh +nightly --ui`.

To update expected output:

```sh
TRYBUILD=overwrite ./scripts/test.sh +nightly --ui
TRYBUILD=overwrite ./scripts/test.sh +stable --ui
```

Review the diff and verify that error messages point clearly to the source of
the problem.

### Docs Site

The documentation site is built with Docusaurus and lives in `website/`.
Before submitting documentation changes, preview them locally:

```sh
cd website
npm install   # first time only
npm start     # dev server with live reload at http://localhost:3000
```

## Code Style
[Code Style]: #code-style

Format all code with `cargo fmt`. No other style conventions are enforced beyond
what the formatter produces.

## Licensing

Any contribution intentionally submitted for inclusion in rkt shall be dual
licensed under the MIT License and Apache License, Version 2.0, without any
additional terms or conditions, unless you explicitly state otherwise.

The website docs are licensed under [separate terms](docs/LICENSE). Contributions
to the website docs shall be licensed under those terms.

[feature request]: https://github.com/rustfoo/rkt/issues/new?labels=request&template=feature-request.yml
