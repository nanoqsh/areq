use crate::{
    body::IntoBody,
    proto::{Error, Protocol, Request, Response, Serve},
};

pub struct Client<P, B>(pub(crate) P::Serve<B>)
where
    P: Protocol + ?Sized,
    B: IntoBody;

impl<P, B> Client<P, B>
where
    P: Protocol,
    B: IntoBody,
{
    #[inline]
    pub async fn send(
        &mut self,
        mut req: Request<B>,
    ) -> Result<Response<<P::Serve<B> as Serve<B>>::Body>, Error> {
        self.0.prepare(&mut req);
        self.0.serve(req).await
    }
}

impl<P, B> Clone for Client<P, B>
where
    P: Protocol<Serve<B>: Clone>,
    B: IntoBody,
{
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
