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

## Seen in M2 (settings and the brand look)

| Where | Idea | In your languages |
|---|---|---|
| `settings.rs` `#[derive(Serialize, Deserialize)]` | **serde**: turn a struct into a file and back | Like `System.Text.Json` / Jackson / Go's `encoding/json` struct tags, but generated at compile time. `#[serde(default)]` means "a missing field takes its default", which is what lets old files load. |
| `settings.rs` `save` | **Atomic write** (temp file + rename) | Same idea as writing to a temp file then `File.Move(..., overwrite)`: readers see the old file or the new one, never half of each. |
| `settings.rs` `Loaded { settings, problem }` | **Returning a value AND a warning** instead of throwing | Errors you can recover from are ordinary data here, not exceptions. |
| `settings.rs` `MIGRATIONS` | **A list of function pointers** (`&[fn(&mut Table)]`) | Like a `List<Action<Table>>` of upgrade steps run in order. |
| `data_dir.rs` `choose(portable, user_profile)` | **Pass the inputs in, so tests can try every case** | A pure function of its arguments is easy to test without touching the real machine. |
| `profile.rs` | **Never-failing constructors**: `Profile::load()` returns a usable value even after errors | Failure is turned into a notice for the user, not a crash. |
| `bitdinghy-app/build.rs` | **Build scripts**: Rust code that runs at compile time | Like an MSBuild task or a Go `go generate` step; here it embeds the icon into the `.exe`. |
| `brand.rs` `include_bytes!` | **Compiling a file into the program** | Like an embedded resource in .NET: no loose files to ship. |
| `main.rs` `shut_down` | **A runtime-context bug** | `tokio::time::timeout(..)` must be created *inside* the runtime. It's like touching a UI control from the wrong thread: it compiles, then fails when run. |

## Seen in M1 (downloading)

| Where | Idea | In your languages |
|---|---|---|
| `engine.rs` `DownloadStatus`, `EntryState` | **Enums that carry data**, handled with `match` | Like a C# `switch` over a sealed class hierarchy, or a Go interface with a type switch, but the compiler checks you covered every case. |
| `engine.rs` `Mutex<...>`, `Arc<Entry>` | **Shared state across threads** | `Arc` = shared reference-counted pointer; `Mutex` = lock that *owns* the data it protects, so you can't touch the data without holding the lock. |
| `engine.rs` `async fn add_checked` | **async/await** | Like C# `async`/`await`: `.await` marks where the task may pause. The runtime (tokio) plays the role of the thread pool. |
| `engine.rs` `runtime.spawn(async move { ... })` | **Background tasks and `move` closures** | Like `Task.Run` or `go func()`. `move` hands the captured variables to the task, so it can outlive the function that started it. |
| `magnet.rs`, `safety.rs` | **`Result` and `?`**, custom error types | No exceptions: a function returns `Ok(value)` or `Err(error)`; `?` means "on error, return it to my caller". |
| `magnet.rs` `let Some(x) = ... else { return ... }` | **`let ... else`** | An early-return guard, like `if (x == null) return ...;` but it also gives you the unwrapped value. |
| `safety.rs` `Platform` argument | **Testing code for another OS** | Passing the platform in as a parameter lets Linux tests check Windows rules. |
| `tests/engine.rs` | **Integration tests** | Files in a crate's `tests/` folder are compiled as separate programs that use only the public API, like a separate test project. |
| `tests/engine.rs` `#[tokio::test]` | **Async tests** | The test body runs inside a tokio runtime. |
| `bitdinghy-app` `ui()` | **Immediate-mode UI** | The whole screen is described again every frame from the latest data; there are no widgets to keep in sync. |

## Seen so far in the skeleton

- `Cargo.toml` at the root: the workspace. `workspace = true` in a crate means "use the version
  chosen once at the root".
- `crates/dinghy-core/src/lib.rs`: constants, a plain function and unit tests (`#[cfg(test)]`).
- `crates/dinghy-cli/src/main.rs`: `#[derive(Parser)]` generates `--help` and `--version`.
- `crates/dinghy-cli/tests/cli.rs`: an integration test that runs the real program.
