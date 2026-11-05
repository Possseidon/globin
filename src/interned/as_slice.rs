use std::{
    array::TryFromSliceError,
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    mem::{ManuallyDrop, forget},
    ops::Deref,
    ptr,
    sync::Arc,
};

use crate::{INTERNERS, Intern, Interned, InternedWeak, Weak};

/// An interned [`[T; N]`](prim@array), but stored as and compatible with [`[T]`](prim@slice).
///
/// While [`Interned<[T; N]>`][Interned] works just fine, there are various advantages in using
/// [`InternedAsSlice<T, N>`] instead:
///
/// - No duplicate storage: [`Interned<[T]>`][Interned] and [`InternedAsSlice<T, N>`] are both
///   stored as [`[T]`](prim@slice)
///   - This also allows [`PartialEq`] between the two
/// - [`Interned`] and [`InternedAsSlice`] can be converted between each other:
///   - [`InternedAsSlice::unsize`] or [`Into`] to unsize into an [`Interned<[T]>`][Interned]
///   - [`Interned::into_array`] to (fallibly) give it a compile-time known size
pub struct InternedAsSlice<T: Intern, const N: usize>(ManuallyDrop<Arc<[T; N]>>);

impl<T: Intern, const N: usize> Drop for InternedAsSlice<T, N> {
    fn drop(&mut self) {
        // SAFETY: self is not used after the Arc has been taken out
        INTERNERS.drop_value(unsafe { ManuallyDrop::take(&mut self.0) } as Arc<[T]>);
    }
}

impl<T: Intern, const N: usize> Clone for InternedAsSlice<T, N> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Hidden from [`fmt::Debug`] as is the case with other smart-pointers like [`Box`] and [`Arc`].
impl<T: Intern + fmt::Debug, const N: usize> fmt::Debug for InternedAsSlice<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.0).fmt(f)
    }
}

impl<T: Intern, const N: usize> fmt::Pointer for InternedAsSlice<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Intern, const N: usize> Default for InternedAsSlice<T, N>
where
    [T; N]: Default,
{
    fn default() -> Self {
        Self::from_owned(Default::default())
    }
}

impl<T: Intern, const N: usize> Hash for InternedAsSlice<T, N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = Arc::as_ptr(&self.0) as *const ();
        stripped_ptr.hash(state);
    }
}

/// Intentionally only between equal `N`s since different `N`s are never equal.
impl<T: Intern, const N: usize> PartialEq for InternedAsSlice<T, N> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: Intern, const N: usize> Eq for InternedAsSlice<T, N> {}

impl<T: Intern, const N: usize> PartialEq<Interned<[T]>> for InternedAsSlice<T, N> {
    fn eq(&self, other: &Interned<[T]>) -> bool {
        other == self
    }
}

impl<T: Intern, const N: usize> PartialEq<Weak<[T]>> for InternedAsSlice<T, N> {
    fn eq(&self, other: &Weak<[T]>) -> bool {
        other == self
    }
}

impl<T: Intern, const N: usize> PartialEq<WeakAsSlice<T, N>> for InternedAsSlice<T, N> {
    fn eq(&self, other: &WeakAsSlice<T, N>) -> bool {
        ptr::addr_eq(self.as_ptr(), other.0.as_ptr())
    }
}

impl<T: Intern + PartialOrd, const N: usize> PartialOrd for InternedAsSlice<T, N> {
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

impl<T: Intern + Ord, const N: usize> Ord for InternedAsSlice<T, N> {
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

impl<T: Intern, const N: usize> Deref for InternedAsSlice<T, N> {
    type Target = [T; N];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Intern, Q: ?Sized, const N: usize> AsRef<Q> for InternedAsSlice<T, N>
where
    [T; N]: AsRef<Q>,
{
    fn as_ref(&self) -> &Q {
        (**self.0).as_ref()
    }
}

impl<T: Intern, const N: usize> From<[T; N]> for InternedAsSlice<T, N> {
    fn from(value: [T; N]) -> Self {
        Self::from_owned(value)
    }
}

impl<T: Intern, const N: usize> From<Box<[T; N]>> for InternedAsSlice<T, N> {
    fn from(value: Box<[T; N]>) -> Self {
        Self::from_box(value)
    }
}

impl<T: Intern + Clone, const N: usize> From<&[T; N]> for InternedAsSlice<T, N> {
    fn from(value: &[T; N]) -> Self {
        Self::from_ref(value)
    }
}

impl<T: Intern + Clone, const N: usize> From<&mut [T; N]> for InternedAsSlice<T, N> {
    fn from(value: &mut [T; N]) -> Self {
        (&*value).into()
    }
}

impl<T: Intern, const N: usize> TryFrom<Box<[T]>> for InternedAsSlice<T, N> {
    type Error = Box<[T]>;

    fn try_from(value: Box<[T]>) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_box)
    }
}

impl<T: Intern, const N: usize> TryFrom<Vec<T>> for InternedAsSlice<T, N> {
    type Error = Vec<T>;

    fn try_from(value: Vec<T>) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_box)
    }
}

impl<'a, T: Intern + Clone, const N: usize> TryFrom<&'a [T]> for InternedAsSlice<T, N> {
    type Error = TryFromSliceError;

    fn try_from(value: &'a [T]) -> Result<Self, Self::Error> {
        value.try_into().map(Self::from_ref)
    }
}

impl<'a, T: Intern + Clone, const N: usize> TryFrom<&'a mut [T]> for InternedAsSlice<T, N> {
    type Error = TryFromSliceError;

    fn try_from(value: &'a mut [T]) -> Result<Self, Self::Error> {
        (&*value).try_into()
    }
}

impl<T: Intern, const N: usize> InternedAsSlice<T, N> {
    /// Wraps an [`Arc`] that is assumed to already be interned as a [`[T]`](prim@slice) in
    /// [`INTERNERS`] in [`InternedAsSlice`].
    pub(super) fn new(value: Arc<[T; N]>) -> Self {
        Self(ManuallyDrop::new(value))
    }

    /// Unwraps the [`Arc`] so it can be transferred into a different interned wrapper.
    #[must_use = "leaks unless the Arc is wrapped in a different wrapper"]
    fn relocate(mut self) -> Arc<[T; N]> {
        // SAFETY: self is not used after the Arc has been taken out and Drop is skipped
        unsafe {
            let arc = ManuallyDrop::take(&mut self.0);
            forget(self);
            arc
        }
    }

    /// Unsizes the array into a slice.
    pub fn unsize(self) -> Interned<[T]> {
        Interned::new(self.relocate())
    }

    /// Creates a new [`WeakAsSlice`] reference to this interned value.
    pub fn downgrade(self) -> WeakAsSlice<T, N> {
        WeakAsSlice(Arc::downgrade(&self.0))
    }

    /// Interns a [`Box`]ed `array` as a slice or returns an already interned value.
    pub fn from_box(array: Box<[T; N]>) -> Self {
        Interned::<[T]>::from_box(array)
            .into_array()
            .expect("slice should have the correct length")
    }

    /// Interns the given `array` as a slice or returns an already interned value.
    ///
    /// The given `array` is dropped if it was already interned.
    pub fn from_owned(array: [T; N]) -> Self {
        Interned::slice_from_array(array)
            .into_array()
            .expect("slice should have the correct length")
    }

    /// Interns the given `array` as a slice by cloning it or returns an already interned value.
    pub fn from_ref(array: &[T; N]) -> Self
    where
        T: Clone,
    {
        Interned::<[T]>::from_ref(array)
            .into_array()
            .expect("slice should have the correct length")
    }
}

pub struct WeakAsSlice<T, const N: usize>(pub(super) std::sync::Weak<[T; N]>);

impl<T, const N: usize> Clone for WeakAsSlice<T, N> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, const N: usize> fmt::Debug for WeakAsSlice<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T, const N: usize> Default for WeakAsSlice<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Hash for WeakAsSlice<T, N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // strip metadata, similar to what ptr::addr_eq does
        let stripped_ptr = self.0.as_ptr() as *const ();
        stripped_ptr.hash(state);
    }
}

impl<T, const N: usize> PartialEq for WeakAsSlice<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }
}

impl<T, const N: usize> Eq for WeakAsSlice<T, N> {}

impl<T: Intern, const N: usize> PartialEq<Interned<[T]>> for WeakAsSlice<T, N> {
    fn eq(&self, other: &Interned<[T]>) -> bool {
        other == self
    }
}

impl<T, const N: usize> PartialEq<Weak<[T]>> for WeakAsSlice<T, N> {
    fn eq(&self, other: &Weak<[T]>) -> bool {
        other == self
    }
}

impl<T: Intern, const N: usize> PartialEq<InternedAsSlice<T, N>> for WeakAsSlice<T, N> {
    fn eq(&self, other: &InternedAsSlice<T, N>) -> bool {
        other == self
    }
}

impl<T, const N: usize> WeakAsSlice<T, N> {
    pub const fn new() -> Self {
        Self(std::sync::Weak::new())
    }
}

impl<T: Intern, const N: usize> InternedWeak for WeakAsSlice<T, N> {
    type Strong = InternedAsSlice<T, N>;

    fn dead(&self) -> bool {
        // strong_count of 1 means the Arc was already deinterned and is about to get dropped.
        // It can no longer be upgraded in this state.
        self.0.strong_count() <= 1
    }

    fn upgrade(&self) -> Option<Self::Strong> {
        if self.dead() {
            return None;
        }

        // Lock [T] since that's how it's stored internally!
        INTERNERS
            .get_mut::<[T], _>(|_| {
                if self.dead() {
                    return None;
                }
                self.0.upgrade()
            })
            .map(InternedAsSlice::new)
    }
}
