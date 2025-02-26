use {
    areq::{
        Handshake,
        http::{Method, Uri},
    },
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

    let proto = match proto.as_str() {
        "http1" => Protocol::Http1,
        "http2" => Protocol::Http2,
        "tls" => Protocol::Tls,
        undefined => {
            eprintln!("undefined http protocol: {undefined}");
            return;
        }
    };

    let uri: Uri = match uri.parse() {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("failed to parse uri: {e}");
            return;
        }
    };

    if let Err(e) = future::block_on(fetch(proto, uri)) {
        eprintln!("io error: {e}");
    }
}

enum Protocol {
    Http1,
    Http2,
    Tls,
}

async fn fetch(proto: Protocol, uri: Uri) -> Result<(), Error> {
    use areq::{
        http1::Http1,
        http2::Http2,
        tls::{Select, Tls},
    };

    match proto {
        Protocol::Http1 => get(Http1::default(), uri).await,
        Protocol::Http2 => get(Http2::default(), uri).await,
        Protocol::Tls => {
            let tls = Tls::with_cert(
                Select(Http1::default(), Http2::default()),
                include_bytes!("../../../certs/cert.pem"),
            )?;

            get(tls, uri).await
        }
    }
}

async fn get<H>(handshake: H, uri: Uri) -> Result<(), Error>
where
    H: Handshake<TcpStream>,
{
    use {
        areq::{Address, Client, Request, Session, body::BodyExt},
        async_net::TcpStream,
        futures_lite::{AsyncBufReadExt, StreamExt, future, io::BufReader},
    };

    let addr = Address::from_uri(&uri)?;
    let io = TcpStream::connect(addr.repr().as_ref()).await?;
    let se = Session { addr, io };

    let (mut client, conn) = handshake.handshake(se).await?;
    let handle_io = async {
        conn.await;
        Ok::<_, Error>(())
    };

    let send_request = async move {
        // create new request with empty body
        let req = Request::new(Method::GET, uri, ());

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
        let lines = BufReader::new(res.body().read()).lines();
        let mut stdout = io::stdout();

        let mut stream = pin::pin!(lines);
        while let Some(line) = stream.try_next().await? {
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
