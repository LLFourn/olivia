use std::fmt::Write;

pub fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::new();
    for byte in bytes {
        write!(hex, "{:02x}", byte).unwrap()
    }
    hex
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_to_hex() {
        let bytes = schnorr_fun::fun::G.to_bytes();
        assert_eq!(
            to_hex(bytes.as_ref()),
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
    }
}
