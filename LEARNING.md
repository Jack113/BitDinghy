# Rust concepts you will meet in this code

A reading guide, not a tutorial. Each row says what the idea is, how it compares to C#, Java or
Go, and why BitDinghy uses it. New rows are added as new ideas appear in the code.

| Idea | In your languages | Why it is here |
|---|---|---|
| **Ownership** | No garbage collector. Each value has exactly one owner and is freed when that owner goes out of scope. Assigning a `String` to another variable *moves* it. | No leaks or double-frees, and no GC pauses. |
| **Borrowing** (`&`, `&mut`) | A read-only reference, or one exclusive writable reference, checked when compiling. | Stops "changed while someone was reading" bugs before the program runs. C#'s "collection was modified" exception becomes a compile error. |
| **`Arc`** | A shared, reference-counted pointer (C++ `shared_ptr`). | Lets several threads share one thing, such as the engine. |
| **Async / tokio** | Like C# `async`/`await` on a thread pool. `.await` marks where a task may pause. | Thousands of network connections without thousands of threads. |
| **Channels** | Like Go channels. | Front-ends and the engine pass messages instead of sharing locks. |
| **`Result` and `?`** | Return values instead of exceptions. `?` means "if this failed, return the error to my caller". | Every failure is visible in the code. |
| **Cargo workspace** | A `.sln` (or Go workspace): several crates (assemblies) built together. `Cargo.toml` is the `.csproj`/`go.mod`. | Our shared library plus two programs. |
| **Crate / module** | Crate = assembly/package. `mod` = namespace. Items are private unless marked `pub`. | The engine crate exposes only what the front-ends need. |
| **`#[derive(...)]` and attributes** | Like C# attributes, but many generate real code at compile time. | `clap` builds the command-line parser from a struct. |
| **Immediate-mode UI** | Not retained widgets. The screen is described from scratch every frame. | How egui works: see `ui()` in `crates/bitdinghy-app/src/main.rs`. |

## Seen so far in the skeleton

- `Cargo.toml` at the root: the workspace. `workspace = true` in a crate means "use the version
  chosen once at the root".
- `crates/dinghy-core/src/lib.rs`: constants, a plain function and unit tests (`#[cfg(test)]`).
- `crates/dinghy-cli/src/main.rs`: `#[derive(Parser)]` generates `--help` and `--version`.
- `crates/dinghy-cli/tests/cli.rs`: an integration test that runs the real program.
