# Rule: Coding Standards & UI Design Guidelines

## Scope
Applies to Rust code, TypeScript/React components, CSS styles, and database migrations.

## Rust Standards
1. **Error Handling:** Use `thiserror` for library/subsystem crates and `anyhow` for binary entrypoints. No unwrap/expect in production code paths.
2. **Concurrency:** Always propagate `tokio_util::sync::CancellationToken` through async loops. Ensure thread safety across CEF message pump threads and Tauri async runtimes.
3. **Database:** Parameterize all queries through `rusqlite`. Never construct SQL queries via string formatting.
4. **Code Quality:** Code must pass `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`.

## TypeScript & React Standards
1. **Type Safety:** Enable strict mode (`strict: true`, `noImplicitAny: true`). No `any` type escapes; use explicit discriminated unions for IPC events.
2. **Aesthetics & Design System:**
   * All UI styling must adhere to the Liquid Glass design language defined in `docs/09-ui/Design_System.md`.
   * Colors: anime-futuristic peach-to-burgundy gradient palette (`#F9DBBD` → `#FFA5AB` → `#DA627D` → `#A53860` → `#450920`).
   * Typography: Inter for browser chrome and UI surfaces; JetBrains Mono for code, DOM trees, network headers, and numbers.
3. **Zero Placeholders:** Never use static dummy data or placeholder mocks where live IPC state or dynamic rendering is expected.
