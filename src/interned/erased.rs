use std::{
    any::Any,
    cmp::Ordering,
    error::Error,
    fmt,
    hash::{Hash, Hasher},
    mem::{ManuallyDrop, forget},
    ops::Deref,
    ptr,
    sync::Arc,
};

use crate::{
    INTERNERS, Intern, InternRef, Interned, Weak, impl_intern_erased, interned::InternedWeak,
};

/// Enables type-erasure of an [`Interned<T>`] in a way where it remains interned as a `T`.
///
/// While [`Interned<dyn Trait>`] technically already works, there are various advantages in using
/// [`InternedErased`] instead:
///
/// - `dyn Trait` itself only has to implement [`InternErased`], not [`Eq`], [`Hash`], etc.
///   - Make sure [`ErasedInterned`] is a supertrait and use [`impl_intern_erased`] to implement it
/// - No duplicate storage: [`Interned<T>`] and [`InternedErased<dyn Trait>`] are both stored as `T`
///   - This also allows [`PartialEq`] between the two
/// - [`Interned`] and [`InternedErased`] can be converted between each other:
///   - [`Interned::erase`] or [`Into`] to erase an [`Interned<T>`] into an [`InternedErased`]
///   - [`InternedErased::downcast`] to get back an [`Interned<T>`]
pub struct InternedErased<A: InternErased + ?Sized>(pub(super) ManuallyDrop<Arc<A>>);

impl<A: InternErased + ?Sized> Drop for InternedErased<A> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        unsafe { ManuallyDrop::take(&mut self.0) }.drop()
    }
}

impl<A: InternErased + ?Sized> Clone for InternedErased<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A: InternErased + fmt::Debug + ?Sized> fmt::Debug for InternedErased<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<A: InternErased + fmt::Display + ?Sized> fmt::Display for InternedErased<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<A: InternErased + fmt::Pointer + ?Sized> fmt::Pointer for InternedErased<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<A: InternErased + Error + ?Sized> Error for InternedErased<A> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

// Default can only be implemented for Sized types, but dyn Trait isn't

impl<A: InternErased + ?Sized> Hash for InternedErased<A> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = Arc::as_ptr(&self.0) as *const ();
        stripped_ptr.hash(state);
    }
}

impl<A: InternErased + ?Sized, B: InternErased + ?Sized> PartialEq<InternedErased<B>>
    for InternedErased<A>
{
    fn eq(&self, other: &InternedErased<B>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

impl<A: InternErased + ?Sized> Eq for InternedErased<A> {}

impl<A: InternErasedFrom<T> + ?Sized, T: Intern + ?Sized> PartialEq<Interned<T>>
    for InternedErased<A>
{
    fn eq(&self, other: &Interned<T>) -> bool {
        other == self
    }
}

impl<A: InternErasedFrom<T> + ?Sized, T: ?Sized> PartialEq<Weak<T>> for InternedErased<A> {
    fn eq(&self, other: &Weak<T>) -> bool {
        other == self
    }
}

impl<A: InternErased + ?Sized, B: InternErased + ?Sized> PartialEq<WeakErased<B>>
    for InternedErased<A>
{
    fn eq(&self, other: &WeakErased<B>) -> bool {
        ptr::addr_eq(Arc::as_ptr(&self.0), other.0.as_ptr())
    }
}

impl<A: InternErased + PartialOrd + ?Sized> PartialOrd for InternedErased<A> {
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

impl<A: InternErased + Ord + ?Sized> Ord for InternedErased<A> {
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

impl<A: InternErased + ?Sized> Deref for InternedErased<A> {
    type Target = A;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<A: InternErased + AsRef<Q> + ?Sized, Q> AsRef<Q> for InternedErased<A> {
    fn as_ref(&self) -> &Q {
        (**self.0).as_ref()
    }
}

impl<A: InternErasedFrom<T> + ?Sized, T: Intern> From<Interned<T>> for InternedErased<A> {
    /// Only works with [`Sized`] types.
    fn from(value: Interned<T>) -> Self {
        Interned::erase(value)
    }
}

impl<A: InternErased + ?Sized> InternedErased<A> {
    /// Wraps an [`Arc`] that is assumed to already be interned in [`INTERNERS`] in [`Interned`].
    pub(super) fn new(value: Arc<A>) -> Self {
        Self(ManuallyDrop::new(value))
    }

    /// Unwraps the [`Arc`] so it can be transferred into a different interned wrapper.
    #[must_use = "leaks unless the Arc is wrapped in a different wrapper"]
    fn relocate(mut self) -> Arc<A> {
        // SAFETY: self is not used after the Arc has been taken out and Drop is skipped
        unsafe {
            let arc = ManuallyDrop::take(&mut self.0);
            forget(self);
            arc
        }
    }

    pub fn from_owned<T: Intern>(value: T) -> Self
    where
        A: InternErasedFrom<T>,
    {
        Interned::from_owned(value).into()
    }

    pub fn from_box<T: Intern>(value: Box<T>) -> Self
    where
        A: InternErasedFrom<T>,
    {
        Interned::from_box(value).into()
    }

    pub fn from_ref<T: InternRef>(value: &T) -> Self
    where
        A: InternErasedFrom<T>,
    {
        Interned::from_ref(value).into()
    }

    /// Creates a new [`WeakErased`] reference to this interned value.
    pub fn downgrade(this: &Self) -> WeakErased<A> {
        WeakErased(Arc::downgrade(&this.0))
    }

    /// Tries to downcast the [`InternedErased`] back into the original [`Interned<T>`].
    pub fn downcast<T: Intern>(this: Self) -> Option<Interned<T>> {
        this.relocate().downcast().map(Interned::new)
    }
}

/// Extends [`Any`] with additional functionality to work with [`InternedErased`].
pub trait ErasedInterned: Any + Send + Sync {
    /// Erases the type so that [`Arc::downcast`] can be called on it.
    fn erase_for_downcast(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;

    /// Notifies [`INTERNERS`] that the value is being dropped.
    fn drop(self: Arc<Self>);
}

impl<T: Intern> ErasedInterned for T {
    fn erase_for_downcast(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn drop(self: Arc<Self>) {
        INTERNERS.drop_value(self)
    }
}

impl_intern_erased!(ErasedInterned);

/// Types that can be used with [`InternedErased<A>`], i.e. `dyn ErasedInterned` and subtraits.
pub trait InternErased: Any + Send + Sync {
    /// Downcasts into a concrete type `T`.
    ///
    /// Used to implements [`InternedErased::downcast`].
    fn downcast<T: Intern>(self: Arc<Self>) -> Option<Arc<T>>;

    /// Notifies [`INTERNERS`] that the value is being dropped.
    ///
    /// Used to implement [`Drop`] for [`InternedErased`].
    fn drop(self: Arc<Self>);
}

/// Extends [`InternErased`] with the capability of erasing a concrete type `T`.
pub trait InternErasedFrom<T: ?Sized>: InternErased {
    /// Erases the given concrete `T` into `Self`.
    ///
    /// Used to implement conversion from [`Interned<T>`] to [`InternedErased`].
    fn erase(value: Arc<T>) -> Arc<Self>;
}

/// [`Weak`] but for [`InternedErased`].
///
/// Requires a separate type since [`InternedWeak::upgrade`] must only be able to upgrade into the
/// same type it was downgraded from.
pub struct WeakErased<A: ?Sized>(pub(super) std::sync::Weak<A>);

impl<A: ?Sized> Clone for WeakErased<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A: ?Sized> fmt::Debug for WeakErased<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

// Default can only be implemented for Sized types, but dyn Trait isn't

impl<A: ?Sized> Hash for WeakErased<A> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = self.0.as_ptr() as *const ();
        stripped_ptr.hash(state);
    }
}

impl<A: ?Sized, B: ?Sized> PartialEq<WeakErased<B>> for WeakErased<A> {
    fn eq(&self, other: &WeakErased<B>) -> bool {
        // can't use Weak::ptr_eq since that requires the same T
        ptr::addr_eq(self.0.as_ptr(), other.0.as_ptr())
    }
}

impl<A: ?Sized> Eq for WeakErased<A> {}

impl<A: InternErasedFrom<T> + ?Sized, T: Intern + ?Sized> PartialEq<Interned<T>> for WeakErased<A> {
    fn eq(&self, other: &Interned<T>) -> bool {
        other == self
    }
}

impl<A: InternErasedFrom<T> + ?Sized, T: ?Sized> PartialEq<Weak<T>> for WeakErased<A> {
    fn eq(&self, other: &Weak<T>) -> bool {
        other == self
    }
}

impl<A: InternErased + ?Sized, B: InternErased + ?Sized> PartialEq<InternedErased<B>>
    for WeakErased<A>
{
    fn eq(&self, other: &InternedErased<B>) -> bool {
        other == self
    }
}

// WeakErased::new can only be implemented for Sized types, but dyn Trait isn't

impl<A: InternErased + ?Sized> InternedWeak for WeakErased<A> {
    type Strong = InternedErased<A>;

    fn dead(&self) -> bool {
        // strong_count of 1 means the Arc was already deinterned and is about to get dropped.
        // It can no longer be upgraded in this state.
        self.0.strong_count() <= 1
    }

    /// Upgrades back into an [`InternedErased`] as long as there is another strong reference to it.
    ///
    /// While [`Weak::upgrade`] can get away with just a write-lock on the interner for its concrete
    /// `T`, this version has to take a global write-lock on all [`INTERNERS`]. The reason is
    /// actually quite straightforward: The concrete type `T` of a [`WeakErased`] can only be known
    /// _after_ upgrading it; but we can only upgrade it _while_ holding a lock on that concrete
    /// `T`s interner. No way around that, so we just have to lock all [`INTERNERS`] for a moment.
    fn upgrade(&self) -> Option<Self::Strong> {
        if self.dead() {
            return None;
        }

        // The only difference from Weak::upgrade; a global write-lock since knowing the concrete T
        // requires upgrading the Arc, but upgrading requires a lock.
        INTERNERS
            .with_global_write_lock(|| {
                if self.dead() {
                    return None;
                }
                self.0.upgrade()
            })
            .map(InternedErased::new)
    }
}
