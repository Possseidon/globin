mod as_slice;
mod erased;

use std::{
    array::TryFromSliceError,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    ffi::{CStr, OsStr},
    fmt,
    hash::{Hash, Hasher},
    mem::{ManuallyDrop, forget},
    ops::Deref,
    path::Path,
    ptr,
    sync::Arc,
};

pub use as_slice::*;
pub use erased::*;

use crate::INTERNERS;

/// A value that is interned in the global [`INTERNERS`] registry.
///
/// # Interning
///
/// "Interned" means there is only one concrete instance for any distinct value - as dictated by
/// [`Eq`] - of type `T`. It is implemented as a plain [`Arc`] that is checked against a global
/// registry of [`INTERNERS`] to prevent any duplicate values.
///
/// This not only saves on memory but also allows for a very fast [`Eq`] and [`Hash`] implementation
/// on [`Interned<T>`] itself. Rather than doing a deep comparison/hash like a plain [`Arc`] would
/// do, it can instead just compare/hash the [`Arc`]'s pointer while preserving the same guarantees
/// as if it were a `T`.
///
/// Nesting [`Interned`] values is not only possible but oftentimes even beneficial thanks to the
/// aforementioned cheap [`Eq`] and [`Hash`].
///
/// # Cleanup
///
/// A value is automatically deinterned once all [`Interned<T>`] for that value are dropped.
///
/// # Not [`Borrow<T>`]
///
/// [`Interned<T>`] does not implement [`Borrow<T>`] since it uses a different [`Hash`]
/// implementation that only hashes the pointer to the interned value. [`Borrow<T>`] requires
/// the same [`Hash`] semantics as `T` itself.
///
/// [`Borrow<T>`]: std::borrow::Borrow
///
/// # Can I interact with the underlying [`Arc`]?
///
/// No, you can neither give your own [`Arc`] to be interned, nor can you get back the [`Arc`] from
/// an existing [`Interned`].
///
/// This crate relies on having full control over all [`Arc`]s used by [`Interned`] to ensure the
/// last [`Interned`] getting dropped also deinterning the value. If other references to the
/// underlying [`Arc`] were to exist in user code they would inhibit this cleanup.
///
/// Another reason for not being able to intern an existing [`Arc`] is the fact that an [`Arc`] can
/// be freely coerced into a different type (e.g. `[T; N]` can be coerced to `[T]` or the opposite
/// via [`TryFrom`]) which can result in different interners sharing values. This too would break
/// cleanup with the current implementation.
pub struct Interned<T: Intern + ?Sized>(ManuallyDrop<Arc<T>>);

impl<T: Intern + ?Sized> Drop for Interned<T> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        INTERNERS.drop_value(unsafe { ManuallyDrop::take(&mut self.0) });
    }
}

impl<T: Intern + ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Hidden from [`fmt::Debug`] as is the case with other smart-pointers like [`Box`] and [`Arc`].
impl<T: Intern + fmt::Debug + ?Sized> fmt::Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<T: Intern + fmt::Display + ?Sized> fmt::Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Intern + ?Sized> fmt::Pointer for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Intern + Error + ?Sized> Error for Interned<T> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl<T: Intern + Default> Default for Interned<T> {
    fn default() -> Self {
        Self::from_owned(Default::default())
    }
}

// Arc::<[T]>::default() and other unsized types can share allocations and must be avoided!

impl<T: Intern + Clone> Default for Interned<[T]> {
    fn default() -> Self {
        Self::from_ref(&[])
    }
}

impl Default for Interned<str> {
    fn default() -> Self {
        Self::from_ref("")
    }
}

impl Default for Interned<Path> {
    fn default() -> Self {
        Self::from_ref(Path::new(""))
    }
}

impl Default for Interned<OsStr> {
    fn default() -> Self {
        Self::from_ref(OsStr::new(""))
    }
}

impl Default for Interned<CStr> {
    fn default() -> Self {
        Self::from_ref(c"")
    }
}

impl<T: Intern + ?Sized> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = Arc::as_ptr(&self.0).cast::<()>();
        stripped_ptr.hash(state);
    }
}

impl<T: Intern + ?Sized> PartialEq for Interned<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Intern + ?Sized> Eq for Interned<T> {}

impl<T: Intern + ?Sized> PartialEq<Weak<T>> for Interned<T> {
    fn eq(&self, other: &Weak<T>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
    }
}

impl<T: Intern + Clone, const N: usize> PartialEq<InternedAsSlice<T, N>> for Interned<[T]> {
    fn eq(&self, other: &InternedAsSlice<T, N>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.as_ptr())
    }
}

impl<T: Intern + Clone, const N: usize> PartialEq<WeakAsSlice<T, N>> for Interned<[T]> {
    fn eq(&self, other: &WeakAsSlice<T, N>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
    }
}

impl<T: Intern, A: InternErasedFrom<T> + ?Sized> PartialEq<InternedErased<A>> for Interned<T> {
    fn eq(&self, other: &InternedErased<A>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

impl<T: Intern, A: InternErasedFrom<T> + ?Sized> PartialEq<WeakErased<A>> for Interned<T> {
    fn eq(&self, other: &WeakErased<A>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
    }
}

impl<T: Intern + PartialOrd + ?Sized> PartialOrd for Interned<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else {
            // dereference to skip Arc::partial_cmp which might also do the pointer comparison
            // (although I don't think it currently does)
            (*self.0).partial_cmp(&*other.0)
        }
    }
}

impl<T: Intern + Ord + ?Sized> Ord for Interned<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        if self == other {
            Ordering::Equal
        } else {
            // dereference to skip Arc::cmp which might also do the pointer comparison (although I
            // don't think it currently does)
            (*self.0).cmp(&*other.0)
        }
    }
}

impl<T: Intern + ?Sized> Deref for Interned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Intern + AsRef<Q> + ?Sized, Q: ?Sized> AsRef<Q> for Interned<T> {
    fn as_ref(&self) -> &Q {
        (**self.0).as_ref()
    }
}

impl<T: Intern> From<T> for Interned<T> {
    fn from(value: T) -> Self {
        Self::from_owned(value)
    }
}

// TODO: Don't implement From<Arc> since it can panic, but UniqueArc will be fine:
// impl<T: Intern + ?Sized> From<UniqueArc<T>> for Interned<T> {
//     fn from(value: UniqueArc<T>) -> Self {
//         Self::from_unique_arc(value)
//     }
// }

// From<Vec> is useless since Arc can't reuse the memory, only Box can

impl<T: Intern + ?Sized> From<&T> for Interned<T> {
    fn from(value: &T) -> Self {
        Self::from_ref(value)
    }
}

impl<T: Intern + ?Sized> From<&mut T> for Interned<T> {
    fn from(value: &mut T) -> Self {
        (&*value).into()
    }
}

impl<T: Intern + Clone, const N: usize> From<[T; N]> for Interned<[T]> {
    fn from(value: [T; N]) -> Self {
        Self::slice_from_array(value)
    }
}

impl<T: Intern + Clone, const N: usize> From<&[T; N]> for Interned<[T]> {
    fn from(value: &[T; N]) -> Self {
        Self::from_ref(&value[..])
    }
}

impl<T: Intern + Clone, const N: usize> From<&mut [T; N]> for Interned<[T]> {
    fn from(value: &mut [T; N]) -> Self {
        (&*value).into()
    }
}

impl<T: Intern + Clone, const N: usize> From<InternedAsSlice<T, N>> for Interned<[T]> {
    fn from(value: InternedAsSlice<T, N>) -> Self {
        value.unsize()
    }
}

// TODO: Don't implement From<Arc<[T]>> since it can panic, but UniqueArc will be fine:
// impl<T: Intern + Clone, const N: usize> TryFrom<UniqueArc<[T]>> for Interned<[T; N]> {
//     type Error = UniqueArc<[T]>;

//     fn try_from(value: UniqueArc<[T]>) -> Result<Self, Self::Error> {
//         value.try_into().map(Self::from_unique_arc)
//     }
// }

impl<T: Intern + Clone, const N: usize> TryFrom<&[T]> for Interned<[T; N]> {
    type Error = TryFromSliceError;

    fn try_from(value: &[T]) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_ref)
    }
}

impl<T: Intern + Clone, const N: usize> TryFrom<&mut [T]> for Interned<[T; N]> {
    type Error = TryFromSliceError;

    fn try_from(value: &mut [T]) -> Result<Self, Self::Error> {
        (&*value).try_into()
    }
}

impl<T: Intern + Clone> FromIterator<T> for Interned<[T]> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::from_unique_arc(Arc::from_iter(iter))
    }
}

impl<T: Intern + ?Sized> Interned<T> {
    /// Creates an [`Interned`] from an already interned [`Arc`].
    #[must_use]
    const fn from_interned(value: Arc<T>) -> Self {
        Self(ManuallyDrop::new(value))
    }

    /// Unwraps the [`Arc`] so it can be transferred into a different interned wrapper.
    #[must_use = "leaks unless the Arc is wrapped in a different wrapper"]
    fn relocate(mut self) -> Arc<T> {
        // SAFETY: self is not used after the Arc has been taken out and Drop is skipped
        unsafe {
            let arc = ManuallyDrop::take(&mut self.0);
            forget(self);
            arc
        }
    }

    /// Creates a new [`Weak`] reference to this interned value.
    #[must_use]
    pub fn downgrade(this: &Self) -> Weak<T> {
        Weak(Arc::downgrade(&this.0))
    }

    /// Interns the given unique [`Arc`].
    ///
    /// # Panics
    ///
    /// Panics if the given [`Arc`] is not unique; consider calling [`Arc::make_mut`] if unsure.
    ///
    /// TODO: Replace the panic with `UniqueArc`.
    #[must_use]
    pub fn from_unique_arc(value: Arc<T>) -> Self {
        assert_eq!(Arc::strong_count(&value), 1, "interned Arc must be unique");
        Self::from_interned(
            INTERNERS
                .get_value(&*value)
                .unwrap_or_else(|| INTERNERS.intern(value)),
        )
    }

    /// Interns the given `value`.
    ///
    /// Prefer [`Self::from_owned`] if you have an owned value and don't need it anymore.
    #[must_use]
    pub fn from_ref(value: &T) -> Self {
        Self::from_interned(
            INTERNERS
                .get_value(value)
                .unwrap_or_else(|| INTERNERS.intern(value.clone_into_unique_arc())),
        )
    }
}

impl<T: Intern> Interned<T> {
    /// Interns the given `value`.
    ///
    /// `value` is dropped if it was already interned.
    #[must_use]
    pub fn from_owned(value: T) -> Self {
        Self::from_interned(
            INTERNERS
                .get_value(&value)
                .unwrap_or_else(|| INTERNERS.intern(Arc::new(value))),
        )
    }

    /// Coerces the value into a type-erased [`InternedErased`].
    ///
    /// The value remains interned as a plain `T`; that's the whole point of [`InternedErased`].
    #[must_use]
    pub fn erase<A: InternErasedFrom<T> + ?Sized>(this: Self) -> InternedErased<A> {
        InternedErased::from_interned(A::erase(this.relocate()))
    }
}

impl<T: Intern + Clone> Interned<[T]> {
    /// Interns a fixed-size array [`[T; N]`](prim@array) into an unsized slice [`[T]`](prim@array).
    ///
    /// [`Interned::from_owned`] cannot be used for this since it would require the given `value` to
    /// be of type [`[T]`](prim@slice) which is unsized.
    ///
    /// [`Interned::from_ref`] and [`Interned::from_unique_arc`] both work since they introduce
    /// indirection but the former may [`Clone`] `T` unnecessarily and the latter requires eagerly
    /// wrapping the array in an [`Arc`].
    ///
    /// Also see [`InternedAsSlice`] which is internally always interned as a slice while retaining
    /// its array interface.
    #[must_use]
    pub fn slice_from_array<const N: usize>(value: [T; N]) -> Self {
        // Turbofish to make 100% sure the value is interned as [T] and **not** as [T; N].
        // Wrapping that in Interned would just unsize the already interned Arc which is wrong!
        Self::from_interned(
            INTERNERS
                .get_value::<[_]>(&value)
                .unwrap_or_else(|| INTERNERS.intern::<[_]>(value.into())),
        )
    }

    /// Tries to coerce the unsized slice into a fixed-size array.
    ///
    /// # Errors
    ///
    /// Returns the original [`Interned`] if the slice doesn't hold exactly `N` items.
    pub fn into_array<const N: usize>(self) -> Result<InternedAsSlice<T, N>, Self> {
        self.relocate()
            .try_into()
            .map(InternedAsSlice::from_interned)
            .map_err(Self::from_interned)
    }
}

/// Types that can be interned via [`Interned`].
///
/// It requires all of the following:
///
/// - [`Hash`] and [`Eq`] to look up existing values in an internal [`HashMap`]
/// - [`Send`] and [`Sync`] since [`INTERNERS`] is shared globally across all threads
/// - `'static` since [`INTERNERS`] is `static`
///
/// It is implemented for all [`Sized`] types that are [`Clone`] and for unsized types in the
/// standard library such as [`str`] and [`[T]`](prim@slice) which should be preferred over owned
/// types such as [`String`] and [`Vec`].
pub trait Intern: Hash + Eq + Send + Sync + 'static {
    /// Clones the value into a uniquely owned [`Arc`] for use by an interner.
    ///
    /// TODO: Return `UniqueArc` instead once stabilized.
    fn clone_into_unique_arc(&self) -> Arc<Self>;
}

// TODO: Once Arc::clone_from_ref and CloneToUninit are stabilized:
// impl<T: CloneToUninit + Hash + Eq + Send + Sync + 'static + ?Sized> Intern for T {
//     fn intern(&self) -> Arc<Self> {
//         Arc::clone_from_ref(self)
//     }
// }

impl<T: Clone + Hash + Eq + Send + Sync + 'static> Intern for T {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        Arc::new(self.clone())
    }
}

impl<T: Clone + Hash + Eq + Send + Sync + 'static> Intern for [T] {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        self.into()
    }
}

impl Intern for str {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        self.into()
    }
}

impl Intern for Path {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        self.into()
    }
}

impl Intern for OsStr {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        self.into()
    }
}

impl Intern for CStr {
    fn clone_into_unique_arc(&self) -> Arc<Self> {
        self.into()
    }
}

/// An [`Interned<T>`] that does not prevent the value from being deinterned.
///
/// It implements both [`Eq`] and [`Hash`] but those implementations come with some caveats:
///
/// Once there are no more [`Interned<T>`] for a value, all remaining [`Weak<T>`] no longer have any
/// connection to that original value. They become their own identity, only comparing equal to
/// [`Weak<T>`] that used to point to the same [`Interned<T>`]. They **do not** compare equal to a
/// newly created [`Interned<T>`] of the same original value.
///
/// While this might make it sound like [`Weak<T>`] shouldn't implement [`Eq`] and [`Hash`] at all,
/// being able to use [`Weak<T>`] as the key in a [`HashMap`] is quite useful. In fact,
/// [`WeakHashMapExt`] provides some extension functions for this exact use-case.
///
/// [`HashMap`]: std::collections::HashMap
pub struct Weak<T: ?Sized>(std::sync::Weak<T>);

impl<T: ?Sized> Clone for Weak<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ?Sized> fmt::Debug for Weak<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Default for Weak<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ?Sized> Hash for Weak<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = self.0.as_ptr().cast::<()>();
        stripped_ptr.hash(state);
    }
}

impl<T: ?Sized> PartialEq for Weak<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl<T: ?Sized> Eq for Weak<T> {}

impl<T: Intern + ?Sized> PartialEq<Interned<T>> for Weak<T> {
    fn eq(&self, other: &Interned<T>) -> bool {
        other == self
    }
}

impl<T: Intern + Clone, const N: usize> PartialEq<InternedAsSlice<T, N>> for Weak<[T]> {
    fn eq(&self, other: &InternedAsSlice<T, N>) -> bool {
        ptr::addr_eq(self.0.as_ptr(), other.as_ptr())
    }
}

impl<T, const N: usize> PartialEq<WeakAsSlice<T, N>> for Weak<[T]> {
    fn eq(&self, other: &WeakAsSlice<T, N>) -> bool {
        ptr::addr_eq(self.0.as_ptr(), other.0.as_ptr())
    }
}

impl<T, A: InternErasedFrom<T> + ?Sized> PartialEq<InternedErased<A>> for Weak<T> {
    fn eq(&self, other: &InternedErased<A>) -> bool {
        ptr::addr_eq(self.0.as_ptr(), Arc::as_ptr(&other.0))
    }
}

impl<T, A: InternErasedFrom<T> + ?Sized> PartialEq<WeakErased<A>> for Weak<T> {
    fn eq(&self, other: &WeakErased<A>) -> bool {
        ptr::addr_eq(self.0.as_ptr(), other.0.as_ptr())
    }
}

impl<T> Weak<T> {
    /// Constructs a new [`Weak<T>`], without allocating any memory.
    #[must_use]
    pub const fn new() -> Self {
        Self(std::sync::Weak::new())
    }
}

impl<T: Intern + ?Sized> InternedWeak for Weak<T> {
    type Strong = Interned<T>;

    fn dead(&self) -> bool {
        // strong_count of 1 means the Arc was already deinterned and is about to get dropped.
        // It can no longer be upgraded in this state.
        self.0.strong_count() <= 1
    }

    fn upgrade(&self) -> Option<Self::Strong> {
        // check for liveness before taking a lock as an optimization
        if self.dead() {
            return None;
        }

        // TypedInterner::drop_value has a brief window between getting the strong_count and
        // removing it from the interner where the strong_count must remain at 2. Taking a lock
        // before upgrading ensures that this can't happen.

        // If this lock comes before drop_value then it will be upgraded and prevent the deintern.
        // If this lock comes right after drop_value before the Arc is fully dropped then the Arc
        // will have a strong_count of 1 which must be checked again.
        INTERNERS
            .get_mut::<T, _>(|_| {
                // check for liveness again since drop_value might have just removed it
                if self.dead() {
                    return None;
                }
                self.0.upgrade()
            })
            .map(Interned::from_interned)
    }
}

/// Abstracts over different types of [`Weak`] references to [`Interned`] values.
pub trait InternedWeak {
    /// The type of a strong reference obtained by calling [`Self::upgrade`].
    type Strong;

    /// Returns whether the [`InternedWeak`] can no longer be upgraded.
    ///
    /// While `true` guarantees that any subsequent [`Self::upgrade`] *will* fail, `false` has no
    /// guarantee that a subsequent [`Self::upgrade`] will succeed since it could die in between.
    #[must_use]
    fn dead(&self) -> bool;

    /// Upgrades back into an [`Self::Strong`] as long as there is another strong reference to it.
    ///
    /// Slightly more expensive than a plain [`std::sync::Weak::upgrade`] since the upgrade must be
    /// performed while holding a write-lock on the interner.
    #[must_use]
    fn upgrade(&self) -> Option<Self::Strong>;
}

/// Extension trait for [`HashMap`]s with [`InternedWeak`] keys.
pub trait WeakHashMapExt {
    /// The type of keys stored in the [`HashMap`].
    type Key: InternedWeak;

    /// The type of value stored in the [`HashMap`].
    type Value;

    /// Removes any entries with a [`InternedWeak::dead`] key, retaining only alive ones.
    ///
    /// It usually makes sense to call this once in a while to both reclaim memory for the dead
    /// entries and improve the overall performance of the map.
    fn retain_alive(&mut self);

    /// Removes any entries which are [`InternedWeak::dead`] or for which `f` returns `false`.
    ///
    /// Prefer [`Self::retain_alive`] if `f` always returns `true` since that requires no locking
    /// (for [`InternedWeak::upgrade`]) and is therefore faster.
    fn retain_alive_and(
        &mut self,
        f: impl FnMut(<Self::Key as InternedWeak>::Strong, &mut Self::Value) -> bool,
    );
}

impl<K: InternedWeak, V, S> WeakHashMapExt for HashMap<K, V, S> {
    type Key = K;
    type Value = V;

    fn retain_alive(&mut self) {
        // implemented manually to skip the upgrade which requires locking
        self.retain(|key, _| !key.dead());
    }

    fn retain_alive_and(
        &mut self,
        mut f: impl FnMut(<Self::Key as InternedWeak>::Strong, &mut Self::Value) -> bool,
    ) {
        self.retain(|key, value| key.upgrade().is_some_and(|key| f(key, value)));
    }
}
