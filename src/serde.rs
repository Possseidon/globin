//! Serialization and deserialization for [`Interned`] and friends.

use std::{fmt, path::Path, sync::Arc};

use serde_core::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use crate::{Intern, InternErased, Interned, InternedAsSlice, InternedErased};

impl<T: Intern + Serialize + ?Sized> Serialize for Interned<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

/// Could have used `T: ?Sized` and `Arc<T>: Deserialize<'de>` to deserialize anything but that
/// would require wrapping [`Sized`] types in an [`Arc`] unnecessarily.
impl<'de, T: Intern + Deserialize<'de>> Deserialize<'de> for Interned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::from_owned)
    }
}

impl<'de, T: Intern + Clone + Deserialize<'de>> Deserialize<'de> for Interned<[T]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Arc::deserialize(deserializer).map(Self::from_unique_arc)
    }
}

impl<'de> Deserialize<'de> for Interned<str> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrVisitor;

        impl<'de> Visitor<'de> for StrVisitor {
            type Value = Interned<str>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Interned<str>, E> {
                Ok(Interned::from_ref(v))
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Interned<str>, E> {
                str::from_utf8(v)
                    .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(v), &self))
                    .and_then(|v| self.visit_str(v))
            }
        }

        deserializer.deserialize_string(StrVisitor)
    }
}

impl<'de> Deserialize<'de> for Interned<Path> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PathVisitor;

        impl<'de> Visitor<'de> for PathVisitor {
            type Value = Interned<Path>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("path string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Interned<Path>, E> {
                Ok(Interned::from_ref(Path::new(v)))
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Interned<Path>, E> {
                str::from_utf8(v)
                    .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(v), &self))
                    .and_then(|v| self.visit_str(v))
            }
        }

        deserializer.deserialize_string(PathVisitor)
    }
}

macro_rules! impl_deserialize_from_arc_for {
    ($T:ty) => {
        impl<'de> Deserialize<'de> for Interned<$T> {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Arc::deserialize(deserializer).map(Self::from_unique_arc)
            }
        }
    };
}

// These could also benefit from Arc-less deserialization, but the implementations are more complex.
impl_deserialize_from_arc_for!(std::ffi::OsStr);
impl_deserialize_from_arc_for!(std::ffi::CStr);

impl<T: Intern + Clone + Serialize, const N: usize> Serialize for InternedAsSlice<T, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

/// Mimics for which [`[T; N]`](prim@array) itself implements [`Deserialize`].
///
/// Namely, `[T; 0]` does not require `T` to be [`Deserialize`] at all and it is only implemented up
/// to `[T; 32]`.
impl<'de, T: Intern + Clone, const N: usize> Deserialize<'de> for InternedAsSlice<T, N>
where
    [T; N]: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <[T; N] as Deserialize<'de>>::deserialize(deserializer).map(Self::from_owned)
    }
}

impl<A: InternErased + Serialize + ?Sized> Serialize for InternedErased<A> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

/// Can be implemented to enable deserialization of [`InternedErased`] types.
///
/// This requires some form of factory that knows all possible types. This cannot be solved through
/// dynamic dispatch alone since for that a dispatchable type would first need to exist.
pub trait DeserializeInternedErased<'de>: InternErased {
    fn deserialize_erased<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<InternedErased<Self>, D::Error>;
}

impl<'de, A: InternErased + DeserializeInternedErased<'de> + ?Sized> Deserialize<'de>
    for InternedErased<A>
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        A::deserialize_erased(deserializer)
    }
}
