#![feature(const_slice_len)]

#[macro_use]
extern crate failure;

use mio::{Events, Poll};
use mio_httpc::{CallBuilder, Httpc, HttpcCfg, SimpleCall};

use failure::Error;

fn do_call(htp: &mut Httpc, poll: &Poll, mut call: SimpleCall) -> Result<String, Error> {
    let mut buf: Option<String> = None;
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

const URLS: &'static [&'static str] = &["http://detectportal.firefox.com/success.txt"];

pub fn download<T>() -> Result<[String; URLS.len()], Error> {
    let poll = Poll::new().unwrap();

    let cfg = match HttpcCfg::certs_from_path(".") {
        Ok(cfg) => cfg,
        Err(_) => Default::default(),
    };
    let mut htp = Httpc::new(10, Some(cfg));

    println!("URLS.len(): {}", URLS.len());

    let mut a = [String::from(""); URLS.len()];
    println!("a.len(): {}", a.len());

    for i in 0..URLS.len() {
        println!("Get {}", URLS[i]);
        let call = CallBuilder::get()
            .url(URLS[i])
            .expect("Invalid url")
            .timeout_ms(10000)
            // .insecure_do_not_verify_domain()
            .simple_call(&mut htp, &poll)
            .expect("Call start failed");
        match do_call(&mut htp, &poll, call) {
            Ok(response_string) => a[i] = response_string,
            Err(e) => return Err(format_err!("{}", e)),
        }

        println!("Open connections = {}", htp.open_connections());
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        match download::<String>() {
            // &'static str
            Ok(responses) => {
                for resp in &responses {
                    assert_eq!(resp, "success\n");
                }
            }
            Err(e) => panic!(e),
        }
        // install().and_then(|response| println!("Response: {:?}", response));

        assert_eq!(2 + 2, 4);
    }
}
