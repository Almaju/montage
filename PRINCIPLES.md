# Montage Coding Principles

These principles guide all code in this project. They come from [The Unwrap](https://almaju.github.io/blog/).

## 1. Alphabetical Ordering

**Sort your code alphabetically unless you have a documented reason not to.**

Applies to:
- Struct fields
- Enum variants
- Function parameters
- Impl methods
- Import statements
- Match arms

When in doubt: A comes before B.

```rust
// ✅ Good
struct User {
    created_at: DateTime,
    email: Email,
    id: UserId,
    name: String,
}

// ❌ Bad - "logical" ordering that only makes sense to you
struct User {
    id: UserId,
    email: Email,
    name: String,
    created_at: DateTime,
}
```

**Exceptions must be documented** in comments.

## 2. Primitives → Newtypes

**Wrap primitive types in domain-specific newtypes.**

A `UserId` is not a `String`. A `Duration` is not a `u64`. The type system should enforce this.

```rust
// ✅ Good
struct UserId(String);
struct Email(String);
struct Timestamp(u64);

fn send_notification(user: UserId, email: Email) -> Result<()>

// ❌ Bad - primitives everywhere
fn send_notification(user_id: String, email: String) -> Result<()>
```

Benefits:
- Compiler catches type confusion
- Validation happens once at construction
- Domain operations live on the type

## 3. Structs Are Architecture

**Obsess over your data structures.**

- Structs are the load-bearing walls of your program
- Prefer composition over inheritance
- Dependencies should flow one direction (no cycles)
- Avoid generic suffixes: `Manager`, `Service`, `Handler`, `Controller`

```rust
// ✅ Good - specific names
struct AudioDecoder { ... }
struct WaveformRenderer { ... }
struct TimelineState { ... }

// ❌ Bad - generic nonsense
struct AudioManager { ... }
struct WaveformService { ... }
struct TimelineController { ... }
```

If a struct gets large, split the impl across files. Don't split the struct.

## 4. Errors as Data

**Treat errors as data, not exceptions.**

- Return `Result<T, E>` for operations that can fail
- Never panic except for invariant violations or init failures
- Use typed error enums for different failure modes
- Pattern match on errors for precise recovery

```rust
// ✅ Good
enum AudioError {
    FileNotFound(PathBuf),
    InvalidFormat(String),
    DecodeFailed { codec: String, reason: String },
}

fn load_audio(path: &Path) -> Result<AudioData, AudioError>

// ❌ Bad
fn load_audio(path: &Path) -> AudioData  // panics on error
```

`.unwrap()` is for tests only. Never in production code.

## 5. Explicit Dependencies

**Dependencies should be obvious from type signatures.**

- Pass dependencies through constructors or function parameters
- No singletons
- No DI frameworks
- No global mutable state

```rust
// ✅ Good - dependencies are explicit
struct AudioPlayer {
    decoder: AudioDecoder,
    output: AudioOutput,
}

impl AudioPlayer {
    fn new(decoder: AudioDecoder, output: AudioOutput) -> Self {
        Self { decoder, output }
    }
}

// ❌ Bad - hidden dependencies
impl AudioPlayer {
    fn new() -> Self {
        Self {
            decoder: AudioDecoder::global(),  // singleton
            output: AUDIO_OUTPUT.lock(),      // global
        }
    }
}
```

If a constructor needs 10+ parameters, the struct is doing too much. Split it.

---

## Quick Reference

| Principle | Rule |
|-----------|------|
| Sorting | Alphabetical unless documented |
| Primitives | Wrap in newtypes |
| Structs | Composition, no generic names |
| Errors | Result types, no panics |
| Dependencies | Explicit in signatures |

When in doubt, ask: "Will this be obvious to someone reading this in 6 months?"
