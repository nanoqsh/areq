use {
    areq::{body::Boxed, http::Uri, prelude::*},
    async_net::TcpStream,
    futures_lite::{future, prelude::*},
    std::{
        env,
        io::{self, Error, Write},
        pin,
    },
};

fn main() {
    let mut args = env::args().skip(1);
    let (Some(proto), Some(uri)) = (args.next(), args.next()) else {
        eprintln!("usage: fetch <proto> <uri>");
        return;
    };

    let uri = match uri.parse() {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("failed to parse uri: {e}");
            return;
        }
    };

    if let Err(e) = future::block_on(fetch(&proto, uri)) {
        eprintln!("io error: {e}");
    }
}

async fn fetch(proto: &str, uri: Uri) -> Result<(), Error> {
    use areq::{
        http1::Http1,
        http2::Http2,
        tls::{Select, Tls},
    };

    match proto {
        "http1" => get(Http1::default(), uri).await,
        "http2" => get(Http2::default(), uri).await,
        "tls" => {
            let tls = Tls::with_cert(
                Select(Http1::default(), Http2::default()),
                include_bytes!("../../../certs/cert.pem"),
            )?;

            get(tls, uri).await
        }
        unknown => {
            eprintln!("unknown protocol: {unknown}");
            Ok(())
        }
    }
}

async fn get<H>(handshake: H, uri: Uri) -> Result<(), Error>
where
    H: Handshake<TcpStream, Boxed<'static>>,
{
    use {
        areq::{Address, Session},
        async_net::TcpStream,
        futures_lite::io::BufReader,
    };

    let addr = Address::from_uri(&uri)?;
    let host = addr.host.to_string();
    let io = TcpStream::connect((host, addr.port)).await?;
    let se = Session { addr, io };

    let (mut client, conn) = handshake.handshake(se).await?;
    let handle_io = async {
        conn.await;
        Ok::<_, Error>(())
    };

    let send_request = async move {
        // create new GET request with empty body
        let res = client.get(uri).await?;

        // print response head
        let version = res.version();
        let status = res.status();
        println!("{version:?} {status}");
        for (name, value) in res.headers() {
            println!("{name}: {value:?}");
        }

        println!();

        // print response body
        let read = pin::pin!(res.read());
        let mut lines = BufReader::new(read).lines();
        let mut stdout = io::stdout();

        while let Some(line) = lines.try_next().await? {
            stdout.write_all(line.as_bytes())?;
            stdout.flush()?;
        }

        Ok(())
    };

    // zip two futures to send a request while connection io is handling concurrently
    future::try_zip(handle_io, send_request)
        .boxed_local() // box large futures
        .await?;

    Ok(())
}
