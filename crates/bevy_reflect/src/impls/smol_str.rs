use crate::{self as bevy_reflect};
use crate::{std_traits::ReflectDefault, ReflectDeserialize, ReflectSerialize};
use bevy_reflect_derive::impl_reflect_value;

impl_reflect_value!(::smol_str::SmolStr(
    Debug,
    Hash,
    PartialEq,
    Default,
    Serialize,
    Deserialize,
));

#[cfg(test)]
mod tests {
    use crate::{FromReflect, PartialReflect};
    use smol_str::SmolStr;

    #[test]
    fn should_partial_eq_smolstr() {
        let a: &dyn PartialReflect = &SmolStr::new("A");
        let a2: &dyn PartialReflect = &SmolStr::new("A");
        let b: &dyn PartialReflect = &SmolStr::new("B");
        assert_eq!(Some(true), a.reflect_partial_eq(a2));
        assert_eq!(Some(false), a.reflect_partial_eq(b));
    }

    #[test]
    fn smolstr_should_from_reflect() {
        let smolstr = SmolStr::new("hello_world.rs");
        let output = <SmolStr as FromReflect>::from_reflect(&smolstr);
        assert_eq!(Some(smolstr), output);
    }
}
