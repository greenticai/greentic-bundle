//! ZIP a sorted entries vector into Vec<u8>. Deterministic.

use crate::errors::PackError;
use std::io::Write;

pub fn zip_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, PackError> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for (name, bytes) in entries {
            zip.start_file(name, options).map_err(zip_err)?;
            zip.write_all(bytes).map_err(|e| PackError::Zip(e.to_string()))?;
        }
        zip.finish().map_err(zip_err)?;
    }
    Ok(buf)
}

fn zip_err(e: zip::result::ZipError) -> PackError {
    PackError::Zip(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let entries = vec![
            ("a.txt".into(), b"hello".to_vec()),
            ("b/c.txt".into(), b"world".to_vec()),
        ];
        let bytes = zip_entries(&entries).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        let mut f = zip.by_name("a.txt").unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn deterministic_bytes() {
        let entries = vec![("a.txt".into(), b"x".to_vec())];
        let a = zip_entries(&entries).unwrap();
        let b = zip_entries(&entries).unwrap();
        assert_eq!(a, b);
    }
}
