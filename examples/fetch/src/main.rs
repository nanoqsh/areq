use {
    areq::http::{uri::Scheme, Method, Uri},
    futures_lite::future,
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
        "http1" => Proto::Http1,
        "http2" => Proto::Http2,
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

    if uri.scheme() != Some(&Scheme::HTTP) {
        eprintln!("only http scheme is supported");
        return;
    }

    if let Err(e) = future::block_on(fetch(proto, uri)) {
        eprintln!("io error: {e}");
    }
}

enum Proto {
    Http1,
    Http2,
}

async fn fetch(proto: Proto, uri: Uri) -> Result<(), Error> {
    use {
        areq::{http1::Http1, http2::Http2, or::Or, Address, Client, Handshake, Request, Session},
        async_net::TcpStream,
        futures_lite::{future, io::BufReader, AsyncBufReadExt, StreamExt},
    };

    let handshake = match proto {
        Proto::Http1 => Or::lhs(Http1::default()),
        Proto::Http2 => Or::rhs(Http2::default()),
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
        let lines = BufReader::new(res.body_reader()).lines();
        let mut stdout = io::stdout();

        let mut stream = pin::pin!(lines);
        while let Some(line) = stream.try_next().await? {
            stdout.write_all(line.as_bytes())?;
            stdout.flush()?;
        }

        Ok(())
    };

    // zip two futures to send a request while connection io is handling concurrently
    future::try_zip(handle_io, send_request).await?;
    Ok(())
}
