use {
    crate::proto::Error,
    http::{header, HeaderValue, Method, Request, Version},
    hyper::{
        body::{Body, Frame, Incoming, SizeHint},
        client::conn::http1,
    },
    std::{
        convert::Infallible,
        pin::Pin,
        task::{Context, Poll},
    },
};

pub struct Connection {
    pub(crate) send: http1::SendRequest<Empty>,
    pub(crate) host_header: HeaderValue,
}

impl Connection {
    pub async fn get_request(&mut self, uri: &str) -> Result<Responce, Error> {
        //  let uri = &url[Position::BeforePath..];

        let req = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .version(Version::HTTP_2)
            .header(header::HOST, &self.host_header)
            .header(header::ACCEPT, "*/*")
            .body(Empty)
            .expect("construct a valid request");

        println!("request: {req:#?}");

        self.send.ready().await?;
        let res = self.send.send_request(req).await?;
        println!("response: {res:#?}");

        Ok(Responce(res))
    }
}

#[derive(Debug)]
pub struct Responce(#[expect(dead_code)] hyper::Response<Incoming>);

#[derive(Debug)]
pub(crate) struct Empty;

impl Body for Empty {
    type Data = &'static [u8];
    type Error = Infallible;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        _: &mut Context,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(None)
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        true
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(0)
    }
}
