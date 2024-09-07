use {
    crate::{
        body::Empty,
        proto::{Error, Fetch, Request, Responce},
    },
    http::{header, HeaderValue, Method, Version},
};

pub struct Connection<F> {
    pub(crate) fetch: F,
    pub(crate) host_header: HeaderValue,
}

impl<F> Connection<F> {
    pub async fn get_request(&mut self, uri: &str) -> Result<Responce, Error>
    where
        F: Fetch,
    {
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
