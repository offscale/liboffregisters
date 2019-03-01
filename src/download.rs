use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use mio_httpc::{CallBuilder, Headers, Httpc, HttpcCfg, SimpleCall};

use mio::{Events, Poll};

use url::Url;

use failure::Error;

use crate::fs::basename;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Fail)]
#[fail(display = "request timed out")]
pub struct RequestTimeoutError; /* {
                                    url: Url,
                                }*/

#[derive(Clone)]
pub struct DownloadResponse<'a> {
    pub status: u16,
    pub headers: Headers<'a>,
    pub raw: Option<Vec<u8>>,
    pub downloaded_to: Option<std::ffi::OsString>,
}

impl<'a> DownloadResponse<'a> {
    fn response_text(&self) -> Result<String, Error> {
        if self.raw.is_some() {
            Ok(String::from_utf8(self.raw.clone().unwrap())?)
        } else {
            Err(format_err!("empty response"))
        }
    }
}

impl<'a> fmt::Display for DownloadResponse<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DownloadResponse {{ status: {}, headers: {}, raw: {:?}, downloaded_to: {:?} }}",
            self.status, self.headers, self.raw, self.downloaded_to
        )
    }
}

impl<'a> fmt::Debug for DownloadResponse<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DownloadResponse {{ status: {:#?}, headers: {}, raw: {:#?}, downloaded_to: {:#?} }}",
            self.status, self.headers, self.raw, self.downloaded_to
        )
    }
}

fn do_call<'a>(
    htp: &mut Httpc,
    poll: &Poll,
    mut call: SimpleCall,
) -> Result<DownloadResponse<'a>, Error> {
    let to = ::std::time::Duration::from_millis(100);
    let mut events = Events::with_capacity(8);

    let last_status: u16;
    let raw: Option<Vec<u8>>;
    /* TODO:
    let last_headers: &'a Headers ;
    */
    'outer: loop {
        poll.poll(&mut events, Some(to))?;
        for cref in htp.timeout().into_iter() {
            if call.is_ref(cref) {
                return Err(RequestTimeoutError.into());
            }
        }

        for ev in events.iter() {
            let cref = htp.event(&ev);

            if call.is_call(&cref) && call.perform(htp, &poll)? {
                let (response, body) = match call.finish() {
                    Some(rb) => Ok(rb),
                    None => Err(format_err!("No response")),
                }?;
                raw = Some(body);
                last_status = response.status;
                break 'outer;
            }
        }
    }

    Ok(DownloadResponse {
        status: last_status,         // last_response.status.clone(),
        headers: Headers::default(), // last_response.headers().clone(),
        raw,
        downloaded_to: None,
    })
}

pub fn download<'a, D>(
    target_dir: Option<D>,
    urls: Vec<Url>,
    upsert: bool,
) -> Result<HashMap<Url, DownloadResponse<'a>>, Error>
where
    D: Into<OsString>,
{
    let mut url2response: HashMap<Url, DownloadResponse> = HashMap::new();

    let dir: Option<OsString> = match target_dir {
        Some(d) => Some(d.into()),
        None => None,
    };
    let dir_is_some = dir.is_some().clone();

    let poll = Poll::new()?;

    let cfg = match HttpcCfg::certs_from_path(".") {
        Ok(cfg) => cfg,
        Err(_) => Default::default(),
    };
    let mut htp = Httpc::new(10, Some(cfg));

    let _base = if dir_is_some {
        dir.unwrap()
    } else {
        OsString::default()
    };
    let base = Path::new(&_base);

    for i in 0..urls.len() {
        let url = &urls[i];
        let url_s = url.clone().into_string();

        let call = CallBuilder::get()
            .url(&url_s)
            .unwrap()
            .timeout_ms(10000)
            .max_response(1024 * 1024 * 20) // 20MB
            // .insecure_do_not_verify_domain()
            .simple_call(&mut htp, &poll)?; // .expect("Call start failed");

        let mut error: Option<Error> = None;
        let download_path: Option<String> = if dir_is_some {
            let p = base
                .join(Path::new(&basename(&url_s)))
                .to_string_lossy()
                .into_owned();
            if p.is_empty() {
                error = Some(format_err!(
                    "Conversion to filename failed for: {:#?}",
                    urls[i]
                ));
                None
            } else {
                Some(p)
            }
        } else {
            None
        };

        let to_file = dir_is_some && download_path.is_some();

        if error.is_some() {
            return Err(error.unwrap());
        } else if dir_is_some && download_path.is_none() {
            return Err(format_err!("No filename detectable from URL"));
        }

        let (k, v) = ({
            let download_path_p_opt: Option<PathBuf> = if to_file {
                Some(PathBuf::from(&download_path.unwrap()))
            } else {
                None
            };

            enum Next<'g> {
                Ok((Url, DownloadResponse<'g>)),
                // Err(Error),
                ServerRequest(Option<PathBuf>),
            }

            match match download_path_p_opt {
                Some(download_path_p) => {
                    if download_path_p.exists() && !upsert {
                        match std::fs::read(&download_path_p) {
                            Ok(raw) => Next::Ok((
                                urls[i].clone(),
                                DownloadResponse {
                                    headers: Headers::default(),
                                    status: 200,
                                    raw: Some(raw),
                                    downloaded_to: Some(download_path_p.into()),
                                },
                            )),
                            Err(e) => {
                                eprintln!(
                                    "Received error while reading, downloading again. Error was: {}",
                                    e);
                                Next::ServerRequest(Some(download_path_p))
                            }
                        }
                    } else {
                        Next::ServerRequest(Some(download_path_p))
                    }
                }
                None => Next::ServerRequest(None),
            } {
                Next::Ok((url, res)) => Ok((url, res)),
                // Next::Err(e) => Err(e),
                Next::ServerRequest(dp_p_opt) => match do_call(&mut htp, &poll, call) {
                    Ok(download_response) => {
                        match if download_response.raw.is_none() {
                            Err(format_err!("Empty response from URL"))
                        } else if to_file {
                            let dp_p = dp_p_opt.unwrap();

                            let victor = download_response.raw.unwrap();
                            std::fs::write(&dp_p, &victor)?;
                            Ok(DownloadResponse {
                                headers: download_response.headers,
                                status: download_response.status,
                                raw: Some(victor),
                                downloaded_to: Some(dp_p.into()),
                            })
                        } else {
                            Ok(download_response)
                        } {
                            Ok(v) => Ok((urls[i].clone(), v)),
                            Err(e) => Err(e),
                        }
                    }
                    Err(e) => Err(e),
                },
            }
        } as Result<(Url, DownloadResponse), Error>)?;

        url2response.insert(k, v);
        // println!("Open connections = {}", htp.open_connections());
    }

    Ok(url2response)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::env::temp_dir;
    use std::ffi::OsString;
    use std::fs::metadata;
    use std::sync::Mutex;

    use std::time::SystemTime;
    use tempfile::Builder;

    #[inline(always)]
    fn urls2urls() -> Vec<Url> {
        URLRESPONSES
            .iter()
            .map(|url_response| Url::parse(url_response.url).unwrap())
            .collect()
    }

    #[inline(always)]
    fn error_handler(error: Error) {
        if error.downcast_ref::<RequestTimeoutError>().is_none() {
            let fail = error.as_fail();
            eprintln!(
                "fail.cause(): {:#?}, fail.backtrace(): {:#?}, fail: {:#?}, name: {:#?}",
                fail.cause(),
                fail.backtrace(),
                fail,
                fail.name()
            );
            panic!(error)
        }
    }

    const URLRESPONSES: &'static [&'static UrlResponse] = &[
        &UrlResponse {
            url: "http://detectportal.firefox.com/success.txt",
            status: 200,
            content: "success\n",
            fname: "success.txt",
        },
        &UrlResponse {
            url: "http://www.msftncsi.com/ncsi.txt",
            status: 200,
            content: "Microsoft NCSI",
            fname: "ncsi.txt",
        },
        &UrlResponse {
            url: "http://connectivitycheck.gstatic.com/generate_204",
            status: 204,
            content: "",
            fname: "generate_204",
        },
        &UrlResponse {
            url: "http://www.apple.com/library/test/success.html",
            status: 200,
            content: "<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>",
            fname: "success.html",
        },
    ];

    struct UrlResponse {
        url: &'static str,
        status: u16,
        content: &'static str,
        fname: &'static str,
    }

    lazy_static! {
        static ref URLS: Vec<Url> = urls2urls();
        static ref URL2CREATED: Mutex<HashMap<Url, std::time::SystemTime>> =
            Mutex::new(HashMap::new());
        static ref TMP_DIR: PathBuf = temp_dir().join(env!("CARGO_PKG_NAME"));
    }

    #[test]
    fn download_to_dir() {
        let urls = urls2urls();
        let tmp_dir = Builder::new()
            .prefix(env!("CARGO_PKG_NAME"))
            .tempdir()
            .unwrap();

        match download(Some(&tmp_dir.path()), urls, false) {
            Ok(url2response) => {
                for &expected_url_response in URLRESPONSES {
                    let url: &Url = &Url::parse(expected_url_response.url).unwrap();
                    assert_eq!(url2response.contains_key(url), true);
                    let actual_response = url2response.get(url).unwrap();
                    assert_eq!(
                        actual_response.downloaded_to.clone().unwrap(),
                        tmp_dir
                            .path()
                            .join(expected_url_response.fname)
                            .into_os_string()
                    );
                    assert_eq!(actual_response.status, expected_url_response.status)
                }
            }
            Err(e) => error_handler(e),
        }
    }

    #[test]
    fn download_cache() {
        let tmp_dir = Builder::new()
            .prefix(env!("CARGO_PKG_NAME"))
            .tempdir()
            .unwrap();
        let tmp_dir_os_string = tmp_dir.into_path().into_os_string();

        fn download_for_cache(dir: &OsString) {
            let urls = urls2urls();
            match download(Some(dir), urls, false) {
                Ok(url2response) => {
                    for &expected_url_response in URLRESPONSES {
                        let url: Url = Url::parse(expected_url_response.url).unwrap();
                        assert_eq!(url2response.contains_key(&url), true);
                        let actual_response = url2response.get(&url).unwrap();

                        let path = Path::new(dir).join(expected_url_response.fname);

                        assert_eq!(path.exists(), true);

                        let path_os_string = path.into_os_string();

                        assert_eq!(
                            actual_response.downloaded_to.clone().unwrap(),
                            path_os_string
                        );
                        if expected_url_response.status == 200 {
                            assert_eq!(actual_response.status, expected_url_response.status);
                        }

                        let path_metadata = metadata(path_os_string).unwrap();

                        match URL2CREATED.lock() {
                            Ok(mut url2created) => {
                                let stored_url2created: Option<&SystemTime> = url2created.get(&url);
                                match stored_url2created {
                                    Some(created) => {
                                        assert_eq!(created, &path_metadata.created().unwrap())
                                    }
                                    None => assert_eq!(
                                        url2created.insert(url, path_metadata.created().unwrap()),
                                        None
                                    ),
                                }
                            }
                            Err(_) => panic!("URL2CREATED couldn't be locked"),
                        }
                    }
                }
                Err(e) => error_handler(e),
            }
        };
        download_for_cache(&tmp_dir_os_string); // Download, filling cache
        download_for_cache(&tmp_dir_os_string); // Try download, find in cache, use that instead
                                                // std::fs::remove_dir_all(TMP_DIR.as_path()).unwrap();
    }

    #[test]
    fn download_to_mem() {
        match download(None as Option<&str>, URLS.to_vec(), false) {
            Ok(url2response) => {
                for &expected_url_response in URLRESPONSES {
                    let url: &Url = &Url::parse(expected_url_response.url).unwrap();
                    assert_eq!(url2response.contains_key(url), true);
                    let actual_response = url2response.get(url).unwrap();
                    assert_eq!(
                        actual_response.response_text().unwrap(),
                        expected_url_response.content
                    );
                    assert_eq!(actual_response.status, expected_url_response.status)
                }
            }
            Err(e) => error_handler(e),
        }
    }
}
