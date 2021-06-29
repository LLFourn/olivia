/// Implements Display, FromStr, Serialize and Deserialize for something that
/// can be represented as a fixed length byte array
#[macro_export]
#[cfg_attr(rustfmt, rustfmt::skip)]
#[doc(hidden)]
macro_rules! impl_fromstr_deserialize {
    (
        name => $name:literal,
        fn from_bytes$(<$($tpl:ident  $(: $tcl:ident)?),*>)?($input:ident : [u8;$len:literal]) ->  Option<$type:path> $block:block
    ) => {

        impl$(<$($tpl $(:$tcl)?),*>)? core::str::FromStr for $type  {
            type Err = $crate::hex::HexError;

            /// Parses the string as hex and interprets tries to convert the
            /// resulting byte array into the desired value.
            fn from_str(hex: &str) -> Result<$type , $crate::hex::HexError> {
                use $crate::hex::hex_val;
                if hex.len() % 2 == 1 {
                    Err($crate::hex::HexError::InvalidHex)
                } else if $len * 2 != hex.len() {
                    Err($crate::hex::HexError::InvalidLength)
                } else {
                    let mut buf = [0u8; $len];

                    for (i, hex_byte) in hex.as_bytes().chunks(2).enumerate() {
                        buf[i] = hex_val(hex_byte[0])? << 4 | hex_val(hex_byte[1])?
                    }

                    let $input = buf;
                    let result = $block;
                    result.ok_or($crate::hex::HexError::InvalidEncoding)
                }
            }
        }

        impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::Deserialize<'de> for $type  {
            fn deserialize<Deser: $crate::serde::Deserializer<'de>>(
                deserializer: Deser,
            ) -> Result<$type , Deser::Error> {

                {
                    if deserializer.is_human_readable() {
                        #[allow(unused_parens)]
                        struct HexVisitor$(<$($tpl),*>)?$((core::marker::PhantomData<($($tpl),*)> ))?;
                        impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::de::Visitor<'de> for HexVisitor$(<$($tpl),*>)? {
                            type Value = $type ;
                            fn expecting(
                                &self,
                                f: &mut core::fmt::Formatter,
                            ) -> core::fmt::Result {
                                write!(f, "a valid {}-byte hex encoded {}", $len, $name)?;
                                Ok(())
                            }

                            fn visit_str<E: $crate::serde::de::Error>(self, v: &str) -> Result<$type , E> {
                                use $crate::hex::HexError::*;
                                <$type  as core::str::FromStr>::from_str(v).map_err(|e| match e {
                                    InvalidLength => E::invalid_length(v.len() / 2, &self),
                                    InvalidEncoding => E::invalid_value($crate::serde::de::Unexpected::Str(v), &self),
                                    InvalidHex => E::custom("invalid hex")
                                })
                            }
                        }

                        #[allow(unused_parens)]
                        return deserializer.deserialize_str(HexVisitor$((core::marker::PhantomData::<($($tpl),*)>))?);
                    }
                }

                {
                    #[allow(unused_parens)]
                    struct BytesVisitor$(<$($tpl),*>)?$((core::marker::PhantomData<($($tpl),*)> ))?;

                    impl<'de, $($($tpl $(: $tcl)?),*)?> $crate::serde::de::Visitor<'de> for BytesVisitor$(<$($tpl),*>)? {
                        type Value = $type ;

                        fn expecting(
                            &self,
                            f: &mut core::fmt::Formatter,
                        ) -> core::fmt::Result {
                            write!(f, "a valid {}-byte encoding of a {}", $len, $name)?;
                            Ok(())
                        }

                        fn visit_seq<A>(self, mut seq: A) -> Result<$type , A::Error>
                        where A: $crate::serde::de::SeqAccess<'de> {

                            let mut $input = [0u8; $len];
                            for i in 0..$len {
                                $input[i] = seq.next_element()?
                                .ok_or_else(|| $crate::serde::de::Error::invalid_length(i, &self))?;
                            }

                            let result = $block;
                            result.ok_or($crate::serde::de::Error::custom(format_args!("invalid byte encoding, expected {}", &self as &dyn $crate::serde::de::Expected)))
                        }
                    }

                    #[allow(unused_parens)]
                    deserializer.deserialize_tuple($len, BytesVisitor$((core::marker::PhantomData::<($($tpl),*)>))?)
                }
            }
        }

    };
}
