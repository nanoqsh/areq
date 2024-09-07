use {
    crate::{
        body::Empty,
        proto::{Error, Fetch, Request, Responce},
        Protocol,
    },
    http::{header, HeaderValue, Method, Version},
};

pub struct Connection<P>
where
    P: Protocol + ?Sized,
{
    pub(crate) fetch: P::Fetch,
    pub(crate) host_header: HeaderValue,
}

impl<P> Connection<P>
where
    P: Protocol,
{
    pub async fn get_request(&mut self, uri: &str) -> Result<Responce, Error> {
        let req = hyper::Request::builder()
            .method(Method::GET)
            .uri(uri)
            .version(Version::HTTP_2)
            .header(header::HOST, &self.host_header)
            .header(header::ACCEPT, "*/*")
            .body(Empty)
            .expect("construct a valid request");

        let req = Request(req);
        println!("request: {req:#?}");

        let res = self.fetch.fetch(req).await?;
        println!("response: {res:#?}");

        Ok(res)
    }
}
