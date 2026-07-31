# Repository rules — bevy_ios_toolkit

Less is more. Explicit is always better than implicit.

## Architecture

- Keep one Rust crate with one cargo feature per native capability and one
  matching Swift Package product.
- Swift exposes `@_cdecl` C-ABI functions called from Rust. Asynchronous native
  results return through polled state or drained event queues, never callbacks
  into Rust.
- Stateful integrations use an iOS backend and a deterministic desktop fake.
  Fire-and-forget integrations remain simple functions.
- Keep native symbols, Rust features, Swift products, and public documentation
  in lockstep. A missing Swift product must fail at link time.
- `demo/` is an excluded consumer crate with a thin XcodeGen iOS shell. It
  exercises public APIs but is not part of the published crate.

## Quality

- Run `make pre-commit-checks`, `make test`, and `make ci` before pushing.
- Verify native bridge changes with a resolved unsigned iOS build in addition
  to Rust tests and the desktop fake.
- Keep examples generic and public: no private identifiers, product names, or
  infrastructure details.
- Add a deterministic fake test for every new polled state or message.

## Workflow

- Work from `master` on one issue branch and open one draft pull request.
- Agents never merge, publish, tag releases, load credentials, or add
  AI-authorship trailers.
- Make is internal automation only. Users invoke Cargo, Swift, XcodeGen, and
  Xcode commands directly.
- A human releases only from clean protected `master` after the exact release
  head passes the local gates.
