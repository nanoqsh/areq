use crate::{
    body::IntoBody,
    proto::{Error, Protocol, Request, Response, Serve},
};

/// Client's incoming body stream.
pub type Incoming<P, B> = <<P as Protocol>::Serve<B> as Serve<B>>::Body;

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
    pub async fn send(&mut self, req: Request<B>) -> Result<Response<Incoming<P, B>>, Error> {
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
