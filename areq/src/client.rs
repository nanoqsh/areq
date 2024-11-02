use crate::{
    proto::{Error, Fetch, Request, Responce},
    Protocol,
};

pub struct Client<P, B>(pub(crate) P::Fetch<B>)
where
    P: Protocol,
    B: areq_h1::Body;

impl<P, B> Client<P, B>
where
    P: Protocol,
    B: areq_h1::Body,
{
    pub async fn send(
        &mut self,
        mut req: Request<B>,
    ) -> Result<Responce<<P::Fetch<B> as Fetch<B>>::Body>, Error> {
        self.0.prepare_request(&mut req);
        let res = self.0.fetch(req).await?;
        Ok(res)
    }
}

impl<P, B> Clone for Client<P, B>
where
    P: Protocol<Fetch<B>: Clone>,
    B: areq_h1::Body,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
