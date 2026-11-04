#![doc = include_str!("../README.md")]

mod interned;
mod interner;
mod registry;
#[cfg(feature = "serde")]
mod serde;

pub use interned::{
    Intern, Interned, InternedWeak, Weak, WeakHashMapExt,
    as_slice::{InternedAsSlice, WeakAsSlice},
    erased::{ErasedInterned, InternErased, InternErasedFrom, InternedErased, WeakErased},
};
pub use interner::InternRef;
pub(crate) use interner::TypedInterner;
pub use registry::{INTERNERS, Interner, InternerRegistry};

/// Calls [`InternerRegistry::rename`] with the given type name [`stringify!`]ed.
///
/// By default interners use [`std::any::type_name`] which uses very verbose names.
#[macro_export]
macro_rules! rename_interner {
    ($T:ty) => {
        $crate::INTERNERS.rename::<$T>(stringify!($T).into())
    };
}

/// Implements [`InternErased`] and [`InternErasedFrom<T>`] for `dyn Trait`.
///
/// This enables this `dyn Trait` to be used in [`InternedErased`].
#[macro_export]
macro_rules! impl_intern_erased {
    ($ErasedInterned:path) => {
        impl $crate::InternErased for dyn $ErasedInterned {
            fn downcast<T: $crate::Intern>(
                self: ::std::sync::Arc<Self>,
            ) -> ::std::option::Option<::std::sync::Arc<T>> {
                self.erase_for_downcast().downcast().ok()
            }

            fn drop(self: ::std::sync::Arc<Self>) {
                self.drop()
            }
        }

        impl<T: $ErasedInterned> $crate::InternErasedFrom<T> for dyn $ErasedInterned {
            fn erase(value: ::std::sync::Arc<T>) -> ::std::sync::Arc<Self> {
                value
            }
        }
    };
}
