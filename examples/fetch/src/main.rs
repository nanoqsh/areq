use {
    areq::url::Url,
    std::{
        env,
        io::{self, Error, Write},
        pin,
    },
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

    if let Err(e) = async_io::block_on(fetch(url)) {
        eprintln!("io error: {e}");
    }
}

async fn fetch(url: Url) -> Result<(), Error> {
    use {
        areq::{
            http::{Method, Uri},
            http2::H2,
            url::Host,
            Address, Protocol, Request, Session,
        },
        async_net::TcpStream,
        futures_lite::{future, io::BufReader, AsyncBufReadExt, StreamExt},
    };

    let host = url.host_str().expect("the url should have a host");
    let port = url.port().unwrap_or(80);
    let se = Session {
        io: TcpStream::connect((host, port)).await?,
        addr: Address {
            host: Host::Domain(host.to_owned()),
            port,
            secure: false,
        },
    };

    let (mut client, conn) = H2::default().handshake(se).await?;
    let handle_io = async {
        // this future will only be complete when `reqs` is dropped
        conn.await;
        Ok::<_, Error>(())
    };

    let uri: Uri = url
        .authority()
        .parse()
        .expect("the url path should be valid");

    let send_request = async move {
        // create new request with empty body
        let req = Request::new(uri, Method::GET, ());

        // print response head
        let res = client.send(req).await?;
        let version = res.version();
        let status = res.status();
        println!("{version:?} {status}");
        for (name, value) in res.headers() {
            println!("{name}: {value:?}");
        }

        println!();

        // print response body
        let body = BufReader::new(res.body_reader());
        let mut lines = pin::pin!(body.lines());
        let mut stdout = io::stdout();
        while let Some(line) = lines.try_next().await? {
            stdout.write_all(line.as_bytes())?;
            stdout.flush()?;
        }

        Ok(())
    };

    // zip two futures to send a request while connection io is handling concurrently
    future::try_zip(handle_io, send_request).await?;
    Ok(())
}
