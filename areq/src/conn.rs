use crate::{
    proto::{Error, Fetch, Request, Responce},
    Protocol,
};

pub struct Requester<P>
where
    P: Protocol,
{
    pub(crate) fetch: P::Fetch,
}

impl<P> Requester<P>
where
    P: Protocol,
{
    pub async fn send(&mut self, mut req: Request) -> Result<Responce, Error> {
        self.fetch.prepare_request(&mut req);
        println!("request: {req:#?}");

        let res = self.fetch.fetch(req).await?;
        println!("response: {res:#?}");

        Ok(res)
    }
}
