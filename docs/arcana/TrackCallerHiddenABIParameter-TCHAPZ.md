# Track Caller Hidden ABI Parameter (@TCHAPZ)

If a Rust function has a `#[track_caller]` attribute, `rustc` adds a hidden `&'static Location` parameter to it.

Like this example from the `chrono` library:

```rust
#[track_caller]
pub const fn seconds(seconds: i64) -> TimeDelta { ... }
```

This, of course, causes complications when a Valen user tries to call it:

```
import rust.chrono.TimeDelta;
exported func main() i64 {
  d = TimeDelta.seconds(42i64);
  return d.num_seconds();
}
```

So, the compiler should remember to insert that pointer parameter: `declare i64 @seconds(i64, ptr)`

And since Valen only does panic=abort, we just pass null for it: `call i64 @seconds(i64 42, ptr null)`
