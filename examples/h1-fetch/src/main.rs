use {
    futures_lite::future,
    std::{
        env,
        io::{self, Error, Write},
    },
    url::Url,
};

fn main() {
    let Some(url) = env::args().nth(1) else {
        println!("usage: fetch <url>");
        return;
    };

    let url: Url = match url.parse() {
        Ok(url) => url,
        Err(e) => {
            eprintln!("failed to parse url: {e}");
            return;
        }
    };

    if url.scheme() != "http" {
        eprintln!("only http scheme is supported");
        return;
    }

    if let Err(e) = future::block_on(fetch(url)) {
        eprintln!("io error: {e}");
    }
}

async fn fetch(url: Url) -> Result<(), Error> {
    use {
        areq_h1::Config,
        async_net::TcpStream,
        futures_lite::future,
        http::{header, HeaderValue, Method, Request, Version},
    };

    let host = url.host_str().expect("the url should have a host");
    let port = url.port().unwrap_or(80);
    let tcp = TcpStream::connect((host, port)).await?;

    let (reqs, conn) = Config::default().handshake(tcp);
    let handle_io = async {
        // this future will only be complete when `reqs` is dropped
        conn.await;
        Ok::<_, Error>(())
    };

    let path = url.path().parse().expect("the url path should be valid");
    let send_request = async move {
        // create new request with empty body
        let mut req = Request::new(());

        // set http method
        *req.method_mut() = Method::GET;

        // set the path
        *req.uri_mut() = path;

        // set http verison
        *req.version_mut() = Version::HTTP_11;

        // set the host header
        let host_value = HeaderValue::from_str(host).expect("the host should be valid");
        req.headers_mut().insert(header::HOST, host_value);

        // print response head
        let res = reqs.send(req).await?;
        let version = res.version();
        let status = res.status();
        println!("{version:?} {status}");
        for (name, value) in res.headers() {
            println!("{name}: {value:?}");
        }

        println!();

        // print response body
        let body = res.body();
        let mut stdout = io::stdout();
        loop {
            let frame = body.frame().await?;
            if frame.is_empty() {
                break;
            }

            stdout.write_all(&frame)?;
            stdout.flush()?;
        }

        Ok(())
    };

    // zip two futures to send a request while connection io is handling concurrently
    future::try_zip(handle_io, send_request).await?;
    Ok(())
}
