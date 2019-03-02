use std::ffi::OsString;
use std::fs::read_dir;
use std::fs::File;
use std::path::Path;

use failure::Error;

use flate2::read::GzDecoder;
use tar::Archive;

use crate::fs::mkdirp;

pub fn untar<D, E>(tarfile: D, extract_dir: Option<E>) -> Result<(), Error>
where
    D: Into<OsString>,
    E: Into<OsString>,
{
    let tfile = tarfile.into();
    let extract_to = match extract_dir {
        Some(d) => d.into(),
        None => OsString::from("."),
    };

    // Good practice? - Should we create parent dir, or is that callers responsibility?
    mkdirp(&extract_to)?;
    match &Path::new(&tfile).parent() {
        Some(parent) => Ok(mkdirp(parent)?),
        None => Err(format_err!("no parent found for {:?}", &tfile)),
    }?;

    let tar_gz = File::open(&tfile)?;
    let tar = GzDecoder::new(tar_gz);

    let mut archive = Archive::new(tar);

    archive.unpack(&extract_to)?;
    Ok(())
}

pub fn untar_all_in_dir<D, E>(input_dir: D, extract_dir: Option<E>) -> Result<(), Error>
where
    D: Into<OsString>,
    E: Into<OsString>,
{
    let input_d = input_dir.into();
    let extract_d = match extract_dir {
        Some(d) => d.into(),
        None => input_d.clone(),
    };

    // Good practice? - Should we create the dir, or is that callers responsibility?
    mkdirp(&extract_d)?;

    for path in read_dir(&input_d)? {
        let p = path?.path();
        let _ = match p.extension() {
            Some(ext) => {
                if ext == "gz" {
                    untar(p, Some(&extract_d))?;
                    Some(())
                } else {
                    None
                }
            }
            None => None,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tempfile::TempDir;

    fn tar<D>(tarfile: D)
    where
        D: Into<OsString>,
    {
        let tar_gz = File::create(tarfile.into()).unwrap();
        let enc = GzEncoder::new(tar_gz, Compression::default());
        let mut tar = tar::Builder::new(enc);
        tar.append_file(file!(), &mut File::open(file!()).unwrap())
            .unwrap();
    }

    #[test]
    fn test_untar() {
        let _tmp_dir: TempDir = tempfile::Builder::new()
            .prefix(env!("CARGO_PKG_NAME"))
            .tempdir()
            .unwrap();

        let tmp_dir = _tmp_dir.into_path().join("test_untar");
        mkdirp(&tmp_dir).unwrap();

        let tarfile = tmp_dir.join("example.tar.gz");
        tar(&tarfile);

        let untar_directory = tmp_dir.join("untar");
        untar(&tarfile, Some(&untar_directory)).unwrap();

        assert_eq!(untar_directory.join(file!()).exists(), true);
    }

    #[test]
    fn test_untar_all_in_dir() {
        let _tmp_dir: TempDir = tempfile::Builder::new()
        .prefix(env!("CARGO_PKG_NAME"))
        .tempdir()
        .unwrap();

        let tmp_dir = _tmp_dir.into_path().join("test_untar_all_in_dir");
        mkdirp(&tmp_dir).unwrap();

        let tarfile = tmp_dir.join("example.tar.gz");
        tar(&tarfile);

        let untar_directory = tmp_dir.join("untar");
        mkdirp(&untar_directory).unwrap();
        untar_all_in_dir(&tmp_dir, Some(&untar_directory)).unwrap();

        assert_eq!(untar_directory.join(file!()).exists(), true);
    }
}
