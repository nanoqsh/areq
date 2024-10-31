use crate::{
    proto::{Error, Fetch, Request, Responce},
    Protocol,
};

pub struct Client<P>(pub(crate) P::Fetch)
where
    P: Protocol;

impl<P> Client<P>
where
    P: Protocol,
{
    pub async fn send(&mut self, mut req: Request) -> Result<Responce<P::Body>, Error> {
        self.0.prepare_request(&mut req);
        let res = self.0.fetch(req).await?;
        Ok(res)
    }
}

impl<P> Clone for Client<P>
where
    P: Protocol<Fetch: Clone>,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
