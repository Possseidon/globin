use std::fmt::Debug;

use globin::{
    ErasedInterned, InternedErased, impl_intern_erased, serde::DeserializeInternedErased,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

trait ErasedExample: ErasedInterned + Debug {
    fn name(&self) -> &str;
}

impl_intern_erased!(ErasedExample);

#[derive(Debug, Hash, PartialEq, Eq)]
struct A;

impl ErasedExample for A {
    fn name(&self) -> &str {
        "A"
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
struct B;

impl ErasedExample for B {
    fn name(&self) -> &str {
        "B"
    }
}

impl Serialize for dyn ErasedExample {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.name().serialize(serializer)
    }
}

impl<'de> DeserializeInternedErased<'de> for dyn ErasedExample {
    fn deserialize_erased<D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<InternedErased<Self>, D::Error> {
        // requires a factory that knows all variants
        match &*String::deserialize(deserializer)? {
            "A" => Ok(InternedErased::from_owned(A)),
            "B" => Ok(InternedErased::from_owned(B)),
            name => Err(de::Error::unknown_variant(name, &["A", "B"])),
        }
    }
}

fn main() {
    let example = InternedErased::<dyn ErasedExample>::from_owned(A);

    let json = serde_json::to_string(&example).unwrap();
    println!("Serialized: {json}");

    let example = serde_json::from_str::<InternedErased<dyn ErasedExample>>(&json).unwrap();
    println!("Deserialized: {example:?}");
}
