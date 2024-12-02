use {
    areq::http::{uri::Scheme, Method, Uri},
    futures_lite::future,
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
        Ok(uri) => uri,
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
        areq::{http2::H2, Address, Protocol, Request, Session},
        async_net::TcpStream,
        futures_lite::{future, io::BufReader, AsyncBufReadExt, StreamExt},
    };

    let addr = Address::from_uri(&uri)?;
    let se = Session {
        io: TcpStream::connect(addr.repr().as_ref()).await?,
        addr,
    };

    let (mut client, conn) = H2::default().handshake(se).await?;
    let handle_io = async {
        conn.await;
        Ok::<_, Error>(())
    };

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
        let mut lines = BufReader::new(res.body_reader()).lines();
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
