use sha2::Digest;
use sha2::Sha256;
use std::str::FromStr;
use tokio::io::AsyncReadExt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sha256Value([u8; 32]);
impl Sha256Value {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn new_from_slice(data: &[u8]) -> Result<Sha256Value, ShaReaderError> {
        let mut d: [u8; 32] = Default::default();
        if data.len() != 32 {
            return Err(ShaReaderError::WrongBytesLength(data.len()));
        }
        d.copy_from_slice(data);
        Ok(Sha256Value(d))
    }

    fn from_hasher(hasher: Sha256) -> Sha256Value {
        let hf = hasher.finalize();
        let mut d: [u8; 32] = Default::default();
        d.copy_from_slice(&hf);
        Sha256Value(d)
    }

    pub fn hash_iter_bytes<'a, I>(data: I) -> Sha256Value
    where
        I: Iterator<Item = &'a [u8]>,
    {
        let _buffer = vec![0; 1024 * 3];
        let mut hasher = Sha256::new();

        for ele in data {
            hasher.update(ele);
        }

        Sha256Value::from_hasher(hasher)
    }

    pub async fn from_path(path: &std::path::Path) -> Result<Sha256Value, std::io::Error> {
        let mut f = tokio::fs::File::open(&path).await?;

        let mut buffer = vec![0; 1024 * 3];
        let mut hasher = Sha256::new();

        // read up to 10 bytes
        let mut n = 1;
        while n > 0 {
            n = f.read(&mut buffer[..]).await?;

            if n > 0 {
                hasher.update(&buffer[0..n]);
            }
        }
        Ok(Sha256Value::from_hasher(hasher))
    }
}

impl From<&[u8]> for Sha256Value {
    fn from(v: &[u8]) -> Self {
        let mut expected_hasher = Sha256::new();
        expected_hasher.update(v);
        Sha256Value::from_hasher(expected_hasher)
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ShaReaderError {
    #[error("String was wrong length for Sha 256 after stripping 0x if present, should be 64 characters, was {1}, string: '{0}'")]
    WrongLength(String, usize),
    #[error("Byte slice was the wrong length was {0}, should be 32")]
    WrongBytesLength(usize),
    #[error("String contained a non-hex compatible character '{1}' for Sha 256: {0}")]
    WrongCharacter(String, char),
}

impl FromStr for Sha256Value {
    type Err = ShaReaderError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        if s.len() != 64 {
            return Err(ShaReaderError::WrongLength(s.to_string(), s.len()));
        }
        let mut buf: [u8; 32] = Default::default();

        let mut buf_idx = 0;
        let mut cur = None;
        for chr in s.chars() {
            let chr_code = chr as i32;
            let num = if (48..=57).contains(&chr_code) {
                chr_code - 48
            } else if (65..=70).contains(&chr_code) {
                chr_code - 65 + 10
            } else if (97..=102).contains(&chr_code) {
                chr_code - 97 + 10
            } else {
                return Err(ShaReaderError::WrongCharacter(s.to_string(), chr));
            };
            let num = num as u8;

            match cur {
                None => cur = Some(num * 16),
                Some(prev) => {
                    let tot = prev + num;
                    buf[buf_idx] = tot;
                    buf_idx += 1;
                    cur = None;
                }
            }
        }
        Ok(Sha256Value(buf))
    }
}
impl std::fmt::Display for Sha256Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0[..] {
            write!(f, "{:02x}", &b)?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for Sha256Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Sha256Value")
            .field(&format!("0x{}", self))
            .finish()
    }
}

#[cfg(test)]
mod tests {

    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    #[test]
    fn test_string_round_trip() {
        let original_str = "7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae";
        let sha_v: Sha256Value =
            Sha256Value::from_str(original_str).expect("Should be able to decode");
        assert_eq!(original_str, sha_v.to_string());
    }

    #[test]
    fn test_debug_print() {
        let original_str = "7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae";
        let sha_v: Sha256Value =
            Sha256Value::from_str(original_str).expect("Should be able to decode");
        let expected_debug =
            "Sha256Value(\"0x7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae\")";
        assert_eq!(expected_debug, format!("{:?}", sha_v));
    }
    #[test]
    fn test_with0x_prefix() {
        let original_str = "0x7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae";
        let sha_v: Sha256Value =
            Sha256Value::from_str(original_str).expect("Should be able to decode");
        assert_eq!(
            "7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae",
            sha_v.to_string()
        );
    }

    #[test]
    fn test_upper_case() {
        let original_str = "0x7A7D983F031CAAE0836FECDF4C22BEA0F1239D34ECEFE83F12E66673B307E7AE";
        let sha_v: Sha256Value =
            Sha256Value::from_str(original_str).expect("Should be able to decode");
        assert_eq!(
            "7a7d983f031caae0836fecdf4c22bea0f1239d34ecefe83f12e66673b307e7ae",
            sha_v.to_string()
        );
    }

    #[test]
    fn test_wrong_length() {
        let original_str = "0x7A7D983F031CAAE0836FECDF4C22BE0F1239D34ECEFE83F12E66673B307E7AE";
        let sha_v: Result<Sha256Value, ShaReaderError> = Sha256Value::from_str(original_str);

        match sha_v {
            Ok(_) => panic!("Should have failed to parse wrong length"),
            Err(ex) => assert_eq!(
                ShaReaderError::WrongLength(
                    "7A7D983F031CAAE0836FECDF4C22BE0F1239D34ECEFE83F12E66673B307E7AE".to_string(),
                    63
                ),
                ex
            ),
        }
    }

    #[test]
    fn test_wrong_char() {
        let original_str = "0x7A7D983F031CAAE08Q6FECDF4C22BE0F1239D34ECEFE83F12E66673B307E7AEF";
        let sha_v: Result<Sha256Value, ShaReaderError> = Sha256Value::from_str(original_str);

        match sha_v {
            Ok(_) => panic!("Should have failed to parse wrong length"),
            Err(ex) => assert_eq!(
                ShaReaderError::WrongCharacter(
                    "7A7D983F031CAAE08Q6FECDF4C22BE0F1239D34ECEFE83F12E66673B307E7AEF".to_string(),
                    'Q'
                ),
                ex
            ),
        }
    }

    #[tokio::test]
    async fn test_from_file() -> Result<(), Box<dyn std::error::Error>> {
        let seed_value = "foobar_baz";
        let seed_value_bytes = seed_value.as_bytes();
        let byte_len = seed_value_bytes.len();

        let mut named_temp_file = NamedTempFile::new()?;

        let mut expected_hasher = Sha256::new();

        for _run_idx in 0..100 {
            let ret = named_temp_file.write(seed_value_bytes)?;
            if ret != byte_len {
                panic!("Didn't write everything to file");
            }
            expected_hasher.update(seed_value_bytes);
        }
        named_temp_file.flush()?;

        let generated = Sha256Value::from_path(named_temp_file.path()).await?;

        let expected_sha256: Sha256Value =
            Sha256Value::new_from_slice(&expected_hasher.finalize())?;

        assert_eq!(expected_sha256, generated);

        Ok(())
    }
}
