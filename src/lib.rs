#![doc = include_str!("../README.md")]

mod interned;
mod interner;
mod registry;
#[cfg(feature = "serde")]
pub mod serde;

pub use interned::*;
pub(crate) use interner::TypedInterner;
pub use interner::*;
pub use registry::*;

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
    ($([$($For:tt)*])? $ErasedInterned:path $(where $($Where:tt)*)?) => {
        impl<$($($For)*)?> $crate::InternErased for dyn $ErasedInterned where $($($Where)*)? {
            fn erase_for_downcast(
                self: ::std::sync::Arc<Self>,
            ) -> ::std::sync::Arc<dyn ::std::any::Any + ::std::marker::Send + ::std::marker::Sync>
            {
                self.erase_for_downcast()
            }

            fn drop(self: ::std::sync::Arc<Self>) {
                self.drop()
            }
        }

        impl<_T: $ErasedInterned, $($($For)*)?> $crate::InternErasedFrom<_T> for dyn $ErasedInterned where $($($Where)*)? {
            fn erase(value: ::std::sync::Arc<_T>) -> ::std::sync::Arc<Self> {
                value
            }
        }
    };
}
