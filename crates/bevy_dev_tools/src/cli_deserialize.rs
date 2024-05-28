use bevy_reflect::{TypeRegistration, TypeRegistry};
use nom::{
    branch::alt, bytes::complete::{is_not, tag, take_while, take_while1}, character::complete::{char, space0}, combinator::{opt, recognize}, multi::many0, sequence::{delimited, preceded}, IResult
};
use serde::{de::{self, value::StringDeserializer, Deserialize, Deserializer, Error, IntoDeserializer, MapAccess, Visitor}, forward_to_deserialize_any};
use std::collections::HashMap;
use std::fmt;
use serde::de::DeserializeSeed;


/// Works only with TypedReflectDeserializer and direct deserialization
struct TypedCliDeserializer<'a> {
    input: &'a str,

}

impl<'a> TypedCliDeserializer<'a> {
    fn from_str(input: &'a str) -> Result<Self, de::value::Error> {
        Ok(Self { input })
    }
}

pub struct CliDeserializer<'a> {
    input: &'a str,
    type_registration: &'a TypeRegistry,
}

impl<'a> CliDeserializer<'a> {
    pub fn from_str(input: &'a str, type_registration: &'a TypeRegistry) -> Result<Self, de::value::Error> {
        Ok(Self { input, type_registration })
    }
}

fn is_not_space(c: char) -> bool {
    c != ' ' && c != '\t' && c != '\n'
}

fn parse_quoted_string(input: &str) -> IResult<&str, &str> {
    recognize(delimited(char('"'), is_not("\""), char('"')))(input)
}

fn parse_ron_value(input: &str) -> IResult<&str, &str> {
    recognize(delimited(char('('), is_not(")"), char(')')))(input)
}


fn parse_value(input: &str) -> IResult<&str, &str> {
    preceded(space0, alt((parse_quoted_string, parse_ron_value, take_while1(is_not_space))))(input)
}

fn parse_argument(input: &str) -> IResult<&str, (&str, Option<&str>)> {
    let (input, _) = space0(input)?;
    if input.starts_with("--") {
        let (input, key) = preceded(tag("--"), take_while1(|c| c != ' '))(input)?;
        let (input, value) = opt(preceded(space0, parse_value))(input)?;
        Ok((input, (key, value)))
    } else {
        let (input, value) = parse_value(input)?;
        Ok((input, (value, None)))
    }
}

fn parse_arguments<'a>(input: &'a str, fields: &'static [&'static str]) -> IResult<&'a str, HashMap<String, &'a str>> {
    let (input, args) = many0(parse_argument)(input)?;
    // println!("{:?}", args);
    let mut positional_index = 0;
    let mut map = HashMap::new();
    for (key, value) in args {
        // println!("{}: {:?}", key, value);
        if value.is_some() {
            map.insert(key.to_string(), value.unwrap());
        } else {
            map.insert(fields[positional_index].to_string(), key);
            positional_index += 1;
        }
    }
    Ok((input, map))
}

struct CliMapVisitor<'a> {
    values: HashMap<String, &'a str>,
    index: usize,
    keys: Vec<String>,
}

impl<'a> CliMapVisitor<'a> {
    fn new(values: HashMap<String, &'a str>) -> Self {
        let keys = values.keys().cloned().collect();
        Self { values, keys, index: 0 }
    }
}

impl<'de> MapAccess<'de> for CliMapVisitor<'de> {
    type Error = de::value::Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de>,
    {
        if self.index < self.keys.len() {
            let key = self.keys[self.index].clone();
            seed.deserialize(StringDeserializer::new(key)).map(Some)
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        if self.index < self.keys.len() {
            let key = self.keys[self.index].clone();
            let value = self.values[&key];
            self.index += 1;
            seed.deserialize(&mut ron::de::Deserializer::from_str(value).unwrap())
                .map_err(|ron_err| de::Error::custom(ron_err.to_string()))
        } else {
            Err(de::Error::custom("Value without a key"))
        }
    }
}

impl<'de> Deserializer<'de> for TypedCliDeserializer<'de> {
    type Error = de::value::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!("deserialize_any not implemented")
    }


    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let (_, values) = parse_arguments(self.input, fields).map_err(|_| de::Error::custom("Parse error"))?;
        // println!("{:?}", values);
        visitor.visit_map(CliMapVisitor::new(values))
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}


impl<'de> Deserializer<'de> for CliDeserializer<'de> {
    type Error = de::value::Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        unimplemented!("deserialize_any not implemented")
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de> {
        let struct_name = self.input.split(' ').next().unwrap();
        let Ok((args, _)) = take_while1::<_, &str, ()>(|c| c != ' ')(self.input) else {
            return Err(de::value::Error::custom("Parse error"));
        };
        // println!("Args: {}", args);

        let mut registration = None;
        for reg in self.type_registration.iter() {
            if let Some(ident) = reg.type_info().type_path_table().ident() {
                if ident.to_lowercase() == struct_name.to_lowercase() {
                    registration = Some(reg);
                    break;
                }
            }
        }

        struct SingleMapDeserializer<'a> {
            args: &'a str,
            type_path: String,
        }

        impl<'de> MapAccess<'de> for SingleMapDeserializer<'de> {
            type Error = de::value::Error;

            fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
            where
                K: de::DeserializeSeed<'de>,
            {
                if self.type_path == "" {
                    Ok(None)
                } else {
                    let res = seed.deserialize(StringDeserializer::new(self.type_path.clone())).map(Some);
                    self.type_path = "".to_string();
                    res
                }
            }

            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
            where
                V: de::DeserializeSeed<'de>,
            {
                seed.deserialize(TypedCliDeserializer::from_str(self.args).unwrap())
            }
        }

        visitor.visit_map(SingleMapDeserializer { args, type_path: registration.unwrap().type_info().type_path().to_string() })
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct struct enum identifier ignored_any
    }
}

#[cfg(test)]
mod tests {
    use bevy_reflect::{prelude::*, serde::*, TypeRegistry};
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize, Default, PartialEq)]
    struct SetGold {
        gold: usize,
    }

    #[derive(Debug, Deserialize, Default, PartialEq)]
    struct TestSimpleArgs {
        arg0: usize,
        arg1: String,
    }
    
    #[test]
    fn single_positional() {
        let input = "100";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = SetGold::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, SetGold { gold: 100 });
    }

    #[test]
    fn single_key() {
        let input = "--gold 100";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = SetGold::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, SetGold { gold: 100 });
    }

    #[test]
    fn multiple_positional() {
        let input = "100 \"200 \"";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = TestSimpleArgs::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, TestSimpleArgs { arg0: 100, arg1: "200 ".to_string() });
    }

    #[test]
    fn multiple_key() {
        let input = "--arg0 100 --arg1 \"200 \"";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = TestSimpleArgs::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, TestSimpleArgs { arg0: 100, arg1: "200 ".to_string() });
    }

    #[test]
    fn mixed_key_positional() {
        let input = "100 --arg1 \"200 \"";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = TestSimpleArgs::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, TestSimpleArgs { arg0: 100, arg1: "200 ".to_string() });
    }

    #[derive(Debug, Deserialize, Default, PartialEq)]
    struct ComplexInput {
        arg0: Option<usize>,
        gold: SetGold,
        text_input: String,
    }

    #[test]
    fn complex_input() {
        let input = "Some(100) --text_input \"Some text\" --gold (gold : 200) ";
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let set_gold = ComplexInput::deserialize(deserializer).unwrap();
        assert_eq!(set_gold, ComplexInput { arg0: Some(100), gold: SetGold { gold: 200 }, text_input: "Some text".to_string() });
    }

    #[derive(Debug, Reflect, PartialEq, Default)]
    pub struct SetGoldReflect {
        pub gold: usize,
    }

    #[derive(Debug, Reflect, PartialEq, Default)]
    #[reflect(Default)]
    struct ReflectMultiArgs {
        arg0: usize,
        arg1: String,
        arg2: SetGoldReflect,
    }

    #[test]
    fn test_typed_reflect_deserialize() { 
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<SetGoldReflect>();
        
        let registration = type_registry
            .get(std::any::TypeId::of::<SetGoldReflect>())
            .unwrap();
        
        let mut reflect_deserializer = TypedReflectDeserializer::new(registration, &type_registry);
        let input = "100";
        
        let mut deserializer = TypedCliDeserializer::from_str(input).unwrap();
        let reflect_value = reflect_deserializer.deserialize(deserializer).unwrap();
        
        let val = SetGoldReflect::from_reflect(reflect_value.as_ref()).unwrap();
        assert_eq!(val, SetGoldReflect { gold: 100 });
    }

    #[test]
    fn test_untyped_reflect_deserialize() { 
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<SetGoldReflect>();
        

        let reflect_deserializer = ReflectDeserializer::new(&type_registry);
        let input = "setgoldreflect 100";
        let deserializer = CliDeserializer::from_str(input, &type_registry).unwrap();
        let reflect_value = reflect_deserializer.deserialize(deserializer).unwrap();
        println!("Reflect value: {:?}", reflect_value);

        let val = SetGoldReflect::from_reflect(reflect_value.as_ref()).unwrap();
        assert_eq!(val, SetGoldReflect { gold: 100 });
    }

    #[test]
    fn test_untyped_reflect_with_key_val() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<SetGoldReflect>();

        let reflect_deserializer = ReflectDeserializer::new(&type_registry);
        let input = "setgoldreflect --gold 100";
        let deserializer = CliDeserializer::from_str(input, &type_registry).unwrap();
        let reflect_value = reflect_deserializer.deserialize(deserializer).unwrap();
        println!("Reflect value: {:?}", reflect_value);
    }

    #[test]
    fn test_untyped_reflect_complex() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<SetGoldReflect>();
        type_registry.register::<ReflectMultiArgs>();

        let reflect_deserializer = ReflectDeserializer::new(&type_registry);
        let input = "ReflectMultiArgs 100 --arg2 (gold : 200) --arg1 \"Some text\"";
        let deserializer = CliDeserializer::from_str(input, &type_registry).unwrap();
        let reflect_value = reflect_deserializer.deserialize(deserializer).unwrap();
        println!("Reflect value: {:?}", reflect_value);

        let val = ReflectMultiArgs::from_reflect(reflect_value.as_ref()).unwrap();
        assert_eq!(val, ReflectMultiArgs { arg0: 100, arg1: "Some text".to_string(), arg2: SetGoldReflect { gold: 200 } });
    }

    #[test]
    fn test_with_complex_default() {
        let mut type_registry = TypeRegistry::default();
        type_registry.register::<SetGoldReflect>();
        type_registry.register::<ReflectMultiArgs>();

        let reflect_deserializer = ReflectDeserializer::new(&type_registry);
        let input = "ReflectMultiArgs 100 --arg2 (gold : 200)";
        let deserializer = CliDeserializer::from_str(input, &type_registry).unwrap();
        let reflect_value = reflect_deserializer.deserialize(deserializer).unwrap();

        let val = ReflectMultiArgs::from_reflect(reflect_value.as_ref()).unwrap();
        assert_eq!(val, ReflectMultiArgs { arg0: 100, arg1: "".to_string(), arg2: SetGoldReflect { gold: 200 } });
    }
}