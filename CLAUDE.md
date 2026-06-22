# CLAUDE.md

Claude and other coding agents should use `AGENTS.md` as the source of truth for this repository.

Read `AGENTS.md` before editing code. In particular:
- preserve audio-thread real-time safety;
- do not add `unwrap()`, `expect()`, or `panic!` in non-test code;
- keep source capability honesty in the UI;
- document every `unsafe` block with `// SAFETY:`;
- prefer `#[expect(lint, reason = "...")]` over broad `#[allow(...)]`;
- keep Cargo, Tauri, and frontend package versions in sync.
