use std::collections::HashMap;
use std::path::Path;

use mio_httpc::{CallBuilder, Headers, Httpc, HttpcCfg, SimpleCall};

use mio::{Events, Poll};

use url::Url;

use crate::fs::basename;
use crate::fs::write_file;
use failure::Error;

#[derive(Debug, Fail)]
#[fail(display = "request timed out")]
pub struct RequestTimeoutError; /* {
    url: Url,
}*/

#[derive(Clone)]
pub struct StatusHeadersText<'a> {
    status: u16,
    headers: Headers<'a>,
    text: String,
}

fn do_call<'a>(
    htp: &mut Httpc,
    poll: &Poll,
    mut call: SimpleCall,
) -> Result<StatusHeadersText<'a>, Error> {
    let mut opt_resp: Option<String>;
    let to = ::std::time::Duration::from_millis(100);
    let mut events = Events::with_capacity(8);

    let last_status: u16;
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
                match String::from_utf8(body.clone()) {
                    Ok(s) => {
                        opt_resp = Some(s);
                        last_status = response.status;
                    }
                    Err(_) => {
                        return Err(format_err!("Non utf8 body sized: {}", body.len()));
                    }
                }
                break 'outer;
            }
        }
    }

    match opt_resp {
        Some(text) => Ok(StatusHeadersText {
            status: last_status,         // last_response.status.clone(),
            headers: Headers::default(), // last_response.headers().clone(),
            text,
        }),
        None => Err(format_err!("Empty response")),
    }
}

pub fn download(
    dir: Option<&str>,
    urls: Vec<Url>,
) -> Result<HashMap<Url, StatusHeadersText>, Error> {
    let mut url2response: HashMap<Url, StatusHeadersText> = HashMap::new();

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
        }

        if download_path.is_none() && dir.is_some() {
            return Err(format_err!("No filename detectable from URL"));
        }

        let (k, v) = (match do_call(&mut htp, &poll, call) {
            Ok(status_headers_text) => {
                match if dir.is_some() && download_path.is_some() {
                    let dp = download_path.unwrap();
                    write_file(Path::new(&dp), status_headers_text.text)?;
                    Ok(StatusHeadersText {
                        headers: status_headers_text.headers,
                        status: status_headers_text.status,
                        text: dp,
                    })
                } else {
                    Ok(status_headers_text)
                } {
                    Ok(v) => Ok((urls[i].clone(), v)),
                    Err(e) => Err(e),
                }
            }
            Err(e) => return Err(e),
        } as Result<(Url, StatusHeadersText), Error>)?;

        url2response.insert(k, v);
        // println!("Open connections = {}", htp.open_connections());
    }

    Ok(url2response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::get_tmpdir;

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
                fail.cause(), fail.backtrace(), fail, fail.name());
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
        let tmp_dir = &*get_tmpdir();
        match download(Some(tmp_dir), urls) {
            Ok(url2response) => {
                for &expected_url_response in URLRESPONSES {
                    let url: &Url = &Url::parse(expected_url_response.url).unwrap();
                    assert_eq!(url2response.contains_key(url), true);
                    let actual_response = url2response.get(url).unwrap();
                    assert_eq!(
                        Some(actual_response.text.as_str()),
                        Path::new(tmp_dir)
                            .join(expected_url_response.fname)
                            .to_str()
                    );
                    assert_eq!(actual_response.status, expected_url_response.status)
                }
            }
            Err(e) => error_handler(e)
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
                    assert_eq!(actual_response.text.as_str(), expected_url_response.content);
                    assert_eq!(actual_response.status, expected_url_response.status)
                }
            }
            Err(e) => error_handler(e)
        }
    }
}
