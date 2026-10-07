//! Typed diagnostic serializer. Maps become key/value pairs, so tuple and enum
//! keys are lossless. Identifier tags permit canonical renumbering offline.
use serde::{
    Serialize,
    ser::{
        self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
        SerializeTupleStruct, SerializeTupleVariant,
    },
};
use serde_json::{Map, Value};

type Error = serde_json::Error;
pub(crate) struct DiagnosticSerializer;

macro_rules! scalar {
    ($($name:ident($ty:ty)),*) => {$(
        fn $name(self, value: $ty) -> Result<Value, Error> { serde_json::to_value(value) }
    )*};
}
impl ser::Serializer for DiagnosticSerializer {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = Sequence;
    type SerializeTuple = Sequence;
    type SerializeTupleStruct = Sequence;
    type SerializeTupleVariant = Sequence;
    type SerializeMap = Pairs;
    type SerializeStruct = Fields;
    type SerializeStructVariant = Fields;
    scalar!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char)
    );
    fn serialize_str(self, v: &str) -> Result<Value, Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Error> {
        serde_json::to_value(v)
    }
    fn serialize_none(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Value, Error> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        v: &'static str,
    ) -> Result<Value, Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        v: &T,
    ) -> Result<Value, Error> {
        let inner = v.serialize(DiagnosticSerializer)?;
        if matches!(name, "ObjectId" | "Timestamp" | "EventId") {
            Ok(Value::Object(Map::from_iter([(format!("${name}"), inner)])))
        } else {
            Ok(inner)
        }
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        v: &T,
    ) -> Result<Value, Error> {
        Ok(Value::Object(Map::from_iter([(
            name.into(),
            v.serialize(DiagnosticSerializer)?,
        )])))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Sequence, Error> {
        Ok(Sequence::new(None))
    }
    fn serialize_tuple(self, _: usize) -> Result<Sequence, Error> {
        Ok(Sequence::new(None))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Sequence, Error> {
        Ok(Sequence::new(None))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Sequence, Error> {
        Ok(Sequence::new(Some(name)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Pairs, Error> {
        Ok(Pairs {
            pairs: vec![],
            key: None,
        })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Fields, Error> {
        Ok(Fields {
            fields: Map::new(),
            variant: None,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Fields, Error> {
        Ok(Fields {
            fields: Map::new(),
            variant: Some(name),
        })
    }
}
pub(crate) struct Sequence {
    values: Vec<Value>,
    variant: Option<&'static str>,
}
impl Sequence {
    fn new(variant: Option<&'static str>) -> Self {
        Self {
            values: vec![],
            variant,
        }
    }
    fn push<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.values.push(v.serialize(DiagnosticSerializer)?);
        Ok(())
    }
    fn finish(self) -> Result<Value, Error> {
        let v = Value::Array(self.values);
        Ok(match self.variant {
            Some(name) => Value::Object(Map::from_iter([(name.into(), v)])),
            None => v,
        })
    }
}
impl SerializeSeq for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.push(v)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
macro_rules! tuple {
    ($trait:ident) => {
        impl $trait for Sequence {
            type Ok = Value;
            type Error = Error;
            fn serialize_field<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
                self.push(v)
            }
            fn end(self) -> Result<Value, Error> {
                self.finish()
            }
        }
    };
}
impl SerializeTuple for Sequence {
    type Ok = Value;
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.push(v)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
tuple!(SerializeTupleStruct);
tuple!(SerializeTupleVariant);
pub(crate) struct Pairs {
    pairs: Vec<Value>,
    key: Option<Value>,
}
impl SerializeMap for Pairs {
    type Ok = Value;
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        self.key = Some(v.serialize(DiagnosticSerializer)?);
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        let key = self
            .key
            .take()
            .ok_or_else(|| <Error as ser::Error>::custom("missing map key"))?;
        self.pairs
            .push(Value::Array(vec![key, v.serialize(DiagnosticSerializer)?]));
        Ok(())
    }
    fn end(self) -> Result<Value, Error> {
        Ok(Value::Object(Map::from_iter([(
            "$map".into(),
            Value::Array(self.pairs),
        )])))
    }
}
pub(crate) struct Fields {
    fields: Map<String, Value>,
    variant: Option<&'static str>,
}
impl Fields {
    fn field<T: Serialize + ?Sized>(&mut self, key: &'static str, v: &T) -> Result<(), Error> {
        self.fields
            .insert(key.into(), v.serialize(DiagnosticSerializer)?);
        Ok(())
    }
    fn finish(self) -> Result<Value, Error> {
        let v = Value::Object(self.fields);
        Ok(match self.variant {
            Some(name) => Value::Object(Map::from_iter([(name.into(), v)])),
            None => v,
        })
    }
}
impl SerializeStruct for Fields {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.field(k, v)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
impl SerializeStructVariant for Fields {
    type Ok = Value;
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        k: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.field(k, v)
    }
    fn end(self) -> Result<Value, Error> {
        self.finish()
    }
}
