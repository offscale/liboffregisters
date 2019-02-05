#![feature(const_slice_len)]

#[macro_use]
extern crate failure;

use std::collections::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use mio::{Events, Poll};
use mio_httpc::{CallBuilder, Httpc, HttpcCfg, SimpleCall};

use url::Url;

use failure::Error;

fn do_call(htp: &mut Httpc, poll: &Poll, mut call: SimpleCall) -> Result<String, Error> {
    let mut buf: Option<String>;
    let to = ::std::time::Duration::from_millis(100);
    let mut events = Events::with_capacity(8);
    'outer: loop {
        poll.poll(&mut events, Some(to)).unwrap();
        for cref in htp.timeout().into_iter() {
            if call.is_ref(cref) {
                return Err(format_err!("Request timed out"));
            }
        }

        for ev in events.iter() {
            let cref = htp.event(&ev);

            if call.is_call(&cref) && call.perform(htp, &poll).expect("Call failed") {
                let (resp, body) = call.finish().expect("No response");
                println!("done req = {}", resp.status);
                for h in resp.headers() {
                    println!("Header = {}", h);
                }
                match String::from_utf8(body.clone()) {
                    Ok(s) => buf = Some(s),
                    Err(_) => {
                        return Err(format_err!("Non utf8 body sized: {}", body.len()));
                    }
                }
                break 'outer;
            }
        }
    }
    match buf {
        Some(s) => Ok(s),
        None => Err(format_err!("Empty response")),
    }
}

//const URLS: &'static [&'static str] = &["http://detectportal.firefox.com/success.txt"];

pub fn basename(path: &str) -> String {
    match path.rsplit(std::path::MAIN_SEPARATOR).next() {
        Some(p) => p.to_string(),
        None => path.into(),
    }
}

pub fn download(dir: Option<&str>, urls: Vec<&str>) -> Result<HashMap<String, String>, Error> {
    let mut url2response: HashMap<String, String> = HashMap::new();

    let poll = Poll::new()?;

    let cfg = match HttpcCfg::certs_from_path(".") {
        Ok(cfg) => cfg,
        Err(_) => Default::default(),
    };
    let mut htp = Httpc::new(10, Some(cfg));

    for i in 0..urls.len() {
        let call = CallBuilder::get()
            .url(urls[i])? // .expect("Invalid url")
            .timeout_ms(10000)
            // .insecure_do_not_verify_domain()
            .simple_call(&mut htp, &poll)?; // .expect("Call start failed");

        let mut error: Option<Error> = None;
        let download_path: Option<String> = if dir.is_some() {
            let uri_opt = match Url::parse(urls[i]) {
                Ok(uri) => Some(uri),
                Err(e) => {
                    error = Some(format_err!("{}", e.to_string()));
                    None
                }
            };
            if uri_opt.is_none() {
                None
            } else {
                let p = Path::new(&dir.unwrap())
                    .join(Path::new(&basename(uri_opt.unwrap().path())))
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

        match do_call(&mut htp, &poll, call) {
            Ok(response_string) => url2response.insert(
                urls[i].into(),
                String::from(if dir.is_some() {
                    let dp = download_path.unwrap();
                    write_file(Path::new(&dp), response_string)?;
                    dp
                } else {
                    response_string
                }),
            ),
            Err(e) => return Err(format_err!("{}", e)),
        };

        println!("Open connections = {}", htp.open_connections());
    }

    Ok(url2response)
}

pub fn write_file(path: &Path, content: String) -> Result<(), Error> {
    let display = path.display();
    let mut file = match File::create(&path) {
        Err(why) => Err(format_err!(
            "couldn't create {}: {:#?}",
            display,
            why.kind()
        )),
        Ok(file) => Ok(file),
    }?;
    match file.write_all(content.as_bytes()) {
        Err(why) => Err(format_err!(
            "couldn't write to {}: {:#?}",
            display,
            why.kind()
        )),
        Ok(_) => Ok(()),
    }
}

pub fn env_or(key: &str, default: std::ffi::OsString) -> std::ffi::OsString {
    match std::env::var_os(key) {
        Some(val) => val,
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    const URLS: &'static [&'static str] = &["http://detectportal.firefox.com/success.txt"];

    #[test]
    fn download_to_dir() {
        let _td = temp_dir();
        let _td_cow = _td.to_string_lossy();
        let tmp_dir = _td_cow.as_ref();
        match download(Some(tmp_dir), URLS.to_vec()) {
            Ok(url2response) => {
                for (url, response) in &url2response {
                    assert_eq!(url, URLS[0]);
                    assert_eq!(Path::new(response), Path::new(tmp_dir).join("success.txt"))
                }
            }
            Err(e) => panic!(e),
        }
    }

    #[test]
    fn download_to_mem() {
        match download(None, URLS.to_vec()) {
            Ok(url2response) => {
                for (url, response) in &url2response {
                    assert_eq!(url, URLS[0]);
                    assert_eq!(response, "success\n")
                }
            }
            Err(e) => panic!(e),
        }
    }
}
