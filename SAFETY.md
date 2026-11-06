# SAFETY

There are a few uses of `unsafe`. They are not technically necessary but should help the optimizer and, maybe surprisingly, keep the code slightly simpler; let me explain:

`Interned<T>` is implemented as an `Arc`:

```rust
struct Interned<T>(Arc<T>);
```

This would work fine as is but: In the current design interners hold a **strong** reference to an `Interned`. This means that the reference count of the `Arc` can never drop to zero by itself once interned.

But one of the goals of this library is automatic cleanup of values once they are no longer referenced. What do we do? We implement `Drop` for `Interned`:

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        INTERNERS.drop_value(&self.0);
    }
}
```

`INTERNERS` checks if the value has a reference count of 2 (one reference from the interner, one from the user-owned `Interned` being dropped) and we're good, right? Well... almost; but this just opened the door for a race-condition.

If two `Interned` to the same value (let's call them `A` and `B`) are dropped simultaneously on two threads then it's possible that:

1. `A` is dropped and calls `drop_value`; `B` still exists, the value remains interned, ok
2. *Before* `A` is fully dropped and decrements the `Arc`s ref-count, `B` too calls `drop_value` - but `A` still exists
3. Neither `drop_value` call actually dropped the value and it remains interned even though it should be dropped

So, where is the underlying issue? In the fact that we would have to decrement the `Arc`'s ref-count while holding a lock. But `Drop` only gives us a `&mut self` and the `Arc` is only dropped (and its ref-count decremented) *after* our drop already exited. So we're out of options? Luckily we're not:

The easiest (and also safe) solution would be to wrap the `Arc` in an `Option`:

```rust
struct Interned<T>(Option<Arc<T>>);
```

Now we can give ownership of the `Arc` to `drop_value` and let *it* take a lock and drop the `Arc` while holding the lock.

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        INTERNERS.drop_value(self.0.take());
    }
}
```

This works perfectly fine and doesn't even introduce additional memory overhead since `Option<Arc<T>>` can be niche-optimized, but: Every time we want to do *anything* with the `Interned` we have to check that the `Option` is `Some`. The optimizer may be able to see that it's only briefly `None` during `Drop`, but there's no guarantee; avoiding these checks entirely would be nice.

Well, there is a way and it's called `ManuallyDrop`:

```rust
struct Interned<T>(ManuallyDrop<Arc<T>>);
```

`ManuallyDrop` inhibits the `Arc`'s `Drop` and gives us various ways of dropping the `Arc` manually. The most convenient being `ManuallyDrop::take`:

```rust
impl<T> Drop for Interned<T> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        INTERNERS.drop_value(unsafe { ManuallyDrop::take(&mut self.0) });

        // doing anything with self.0 here may result in undefined behavior
    }
}
```

This converts the `ManuallyDrop<Arc<T>>` back into a plain `Arc<T>`. This is however where we need `unsafe` since the compiler can no longer guarantee that we don't do anything with `self` after we took it out.

Similar `unsafe` blocks can be found in `InternedAsSlice` and `InternedErased` since they use a coerced `Arc` of a different type rather than just wrapping `Interned`.

To implement `Clone`-free coercion between different interned types an additional `unsafe` block is necessary to take the `Arc` out of the source type so it can be relocated into a coerced wrapper. Each type has a `relocate` function that extracts the `Arc`:

```rust
#[must_use = "leaks unless the Arc is wrapped in a different wrapper"]
fn relocate(mut self) -> Arc<T> {
    // SAFETY: self is not used after the Arc has been taken out and Drop is skipped
    unsafe {
        let arc = ManuallyDrop::take(&mut self.0);
        forget(self);
        arc
    }
}
```

The safe `ManuallyDrop::into_inner` sounds like it should be able to do this, but it requires destructuring `Self` to take out the `ManuallyDrop` which is (rightfully) prevented by the manual `Drop` implementation. The `forget` right after `ManuallyDrop::take` is very important as it prevents `Drop` from calling `ManuallDrop::take` a second time at the end of the function.

Care has to be taken with the returned `Arc` since it must be **unconditionally** wrapped in some `Interned` type again. If the `Arc` gets dropped without a wrapper the value can remain interned despite no longer having any reference, effectively leaving an orphaned interner entry until an equivalent value is interned again in the future, if that ever happens.
