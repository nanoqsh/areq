use {
    futures_lite::future,
    http::{Uri, uri::Scheme},
    std::{
        env,
        io::{self, Error, Write},
    },
};

fn main() {
    let Some(uri) = env::args().nth(1) else {
        println!("usage: fetch <uri>");
        return;
    };

    let uri: Uri = match uri.parse() {
        Ok(url) => url,
        Err(e) => {
            eprintln!("failed to parse uri: {e}");
            return;
        }
    };

    if uri.scheme() != Some(&Scheme::HTTP) {
        eprintln!("only http scheme is supported");
        return;
    }

    if let Err(e) = future::block_on(fetch(uri)) {
        eprintln!("io error: {e}");
    }
}

async fn fetch(uri: Uri) -> Result<(), Error> {
    use {
        areq_h1::Config,
        async_net::TcpStream,
        futures_lite::future,
        http::{HeaderValue, Method, Request, Version, header},
    };

    let host = uri.host().expect("the url should have a host");
    let port = uri.port().map_or(80, |port| port.as_u16());
    let tcp = TcpStream::connect((host, port)).await?;

    let (reqs, conn) = Config::default().handshake(tcp);
    let handle_io = async {
        // this future will only be complete when `reqs` is dropped
        conn.await;
        Ok::<_, Error>(())
    };

    let path = uri.path().parse().expect("the url path should be valid");
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
        let mut res = reqs.send(req).await?;
        let version = res.version();
        let status = res.status();
        println!("{version:?} {status}");
        for (name, value) in res.headers() {
            println!("{name}: {value:?}");
        }

        println!();

        // print response body
        let body = res.body_mut();
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
