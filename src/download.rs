use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use mio_httpc::{CallBuilder, Headers, Httpc, HttpcCfg, SimpleCall};

use mio::{Events, Poll};

use url::Url;

use failure::Error;

use crate::fs::basename;

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

pub fn download<'a>(
    dir: Option<&std::ffi::OsString>,
    urls: Vec<Url>,
) -> Result<HashMap<Url, DownloadResponse<'a>>, Error> {
    let mut url2response: HashMap<Url, DownloadResponse> = HashMap::new();

    let poll = Poll::new()?;

    let cfg = match HttpcCfg::certs_from_path(".") {
        Ok(cfg) => cfg,
        Err(_) => Default::default(),
    };
    let mut htp = Httpc::new(10, Some(cfg));

    for i in 0..urls.len() {
        let url = &urls[i];
        let url_s = url.clone().into_string();

        let call = CallBuilder::get()
            .url(&url_s)
            .unwrap()
            .timeout_ms(10000)
            // .insecure_do_not_verify_domain()
            .simple_call(&mut htp, &poll)?; // .expect("Call start failed");

        let mut error: Option<Error> = None;
        let download_path: Option<String> = if dir.is_some() {
            let p = Path::new(&dir.unwrap())
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

        if error.is_some() {
            return Err(error.unwrap());
        } else if download_path.is_none() && dir.is_some() {
            return Err(format_err!("No filename detectable from URL"));
        }

        let (k, v) = (match do_call(&mut htp, &poll, call) {
            Ok(download_response) => {
                match if download_response.raw.is_none() {
                    return Err(format_err!("Empty response from URL"));
                } else if dir.is_some() && download_path.is_some() {
                    let dp = download_path.unwrap();

                    // let path = Path::new(&dp);

                    let victor = download_response.raw.unwrap();
                    std::fs::write(&dp, &victor)?;
                    Ok(DownloadResponse {
                        headers: download_response.headers,
                        status: download_response.status,
                        raw: Some(victor),
                        downloaded_to: Some(dp.into()),
                    })
                } else {
                    Ok(download_response)
                } {
                    Ok(v) => Ok((urls[i].clone(), v)),
                    Err(e) => Err(e),
                }
            }
            Err(e) => return Err(e),
        } as Result<(Url, DownloadResponse), Error>)?;

        url2response.insert(k, v);
        // println!("Open connections = {}", htp.open_connections());
    }

    Ok(url2response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::temp_dir_osstring;

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
    }

    #[test]
    fn download_to_dir() {
        let urls = urls2urls();
        let tmp_dir = temp_dir_osstring();
        match download(Some(&tmp_dir), urls) {
            Ok(url2response) => {
                for &expected_url_response in URLRESPONSES {
                    let url: &Url = &Url::parse(expected_url_response.url).unwrap();
                    assert_eq!(url2response.contains_key(url), true);
                    let actual_response = url2response.get(url).unwrap();
                    assert_eq!(
                        actual_response.downloaded_to.clone().unwrap(),
                        Path::new(&tmp_dir)
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
    fn download_to_mem() {
        match download(None, URLS.to_vec()) {
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
