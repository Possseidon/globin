use serde_core::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use crate::{Intern, InternErased, Interned, InternedAsSlice, InternedErased};

impl<T: Intern + Serialize + ?Sized> Serialize for Interned<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (**self).serialize(serializer)
    }
}

/// Could have used `T: ?Sized` and `Box<T>: Deserialize<'de>` to deserialize anything but that
/// would require [`Box`]ing [`Sized`] types unnecessarily.
impl<'de, T: Intern + Deserialize<'de>> Deserialize<'de> for Interned<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::from_owned)
    }
}

impl<'de, T: Intern + Deserialize<'de>> Deserialize<'de> for Interned<[T]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Box::deserialize(deserializer).map(Self::from_box)
    }
}

impl<'de> Deserialize<'de> for Interned<str> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrVisitor;

        impl<'de> Visitor<'de> for StrVisitor {
            type Value = Interned<str>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Interned<str>, E> {
                Ok(Interned::from_ref(v))
            }

            fn visit_string<E: de::Error>(self, v: String) -> Result<Interned<str>, E> {
                Ok(if v.len() == v.capacity() {
                    Interned::from_box(v.into_boxed_str())
                } else {
                    Interned::from_ref(&v)
                })
            }

            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Interned<str>, E> {
                str::from_utf8(v)
                    .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(v), &self))
                    .and_then(|v| self.visit_str(v))
            }

            fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Interned<str>, E> {
                String::from_utf8(v)
                    .map_err(|error| {
                        de::Error::invalid_value(de::Unexpected::Bytes(&error.into_bytes()), &self)
                    })
                    .and_then(|v| self.visit_string(v))
            }
        }

        deserializer.deserialize_str(StrVisitor)
    }
}

macro_rules! impl_deserialize_from_box_for {
    ($T:ty) => {
        impl<'de> Deserialize<'de> for Interned<$T> {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Box::deserialize(deserializer).map(Self::from_box)
            }
        }
    };
}

// TODO: Do the same thing for Path as for str
impl_deserialize_from_box_for!(std::path::Path);

// TODO: Can I do the str thing for OsStr?
impl_deserialize_from_box_for!(std::ffi::OsStr);

// TODO: Can I do the str thing for CStr?
impl_deserialize_from_box_for!(std::ffi::CStr);

impl<T: Intern + Serialize, const N: usize> Serialize for InternedAsSlice<T, N> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

/// Mimics how [`[T; N]`](prim@array) itself implements [`Deserialize`].
///
/// Namely, `[T; 0]` does not require `T` to be [`Deserialize`] at all and it's only implemented up
/// to `[T; 32]`.
impl<'de, T: Intern, const N: usize> Deserialize<'de> for InternedAsSlice<T, N>
where
    [T; N]: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <[T; N] as Deserialize<'de>>::deserialize(deserializer).map(Self::from_owned)
    }
}

// InternedErased<A> does not support (de)serialization. While serialization is technically possible
// by having users implement Serialize on their `dyn Trait`, doing the same for Deserialize is not
// enough. Deserialize needs to intern the deserialized value which requires knowing the concrete
// type T which is still erased.

impl<A: InternErased + Serialize + ?Sized> Serialize for InternedErased<A> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (**self).serialize(serializer)
    }
}

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
