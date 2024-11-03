use crate::{
    proto::{Error, Request, Responce, Serve},
    Protocol,
};

pub struct Client<P, B>(pub(crate) P::Serve<B>)
where
    P: Protocol + ?Sized,
    B: areq_h1::IntoBody;

impl<P, B> Client<P, B>
where
    P: Protocol,
    B: areq_h1::IntoBody,
{
    pub async fn send(
        &mut self,
        mut req: Request<B>,
    ) -> Result<Responce<<P::Serve<B> as Serve<B>>::Body>, Error> {
        self.0.prepare(&mut req);
        self.0.serve(req).await
    }
}

impl<P, B> Clone for Client<P, B>
where
    P: Protocol<Serve<B>: Clone>,
    B: areq_h1::IntoBody,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
