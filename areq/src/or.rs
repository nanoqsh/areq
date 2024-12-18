use {
    crate::proto::{Client, Error, Handshake, Request, Response, Session},
    areq_body::IntoBody,
    futures_lite::{AsyncRead, AsyncWrite, Stream},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    },
};

pin_project_lite::pin_project! {
    #[project = PinnedOr]
    pub enum Or<L, R> {
        Lhs {
            #[pin]
            l: L,
        },
        Rhs {
            #[pin]
            r: R,
        },
    }
}

impl<L, R> Or<L, R> {
    pub fn lhs(l: L) -> Self {
        Self::Lhs { l }
    }

    pub fn rhs(r: R) -> Self {
        Self::Rhs { r }
    }
}

impl<L, R> Handshake for Or<L, R>
where
    L: Handshake,
    R: Handshake,
{
    type Client<B>
        = Or<L::Client<B>, R::Client<B>>
    where
        B: IntoBody;

    async fn handshake<I, B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        I: AsyncRead + AsyncWrite,
        B: IntoBody,
    {
        match self {
            Self::Lhs { l } => {
                let (client, conn) = l.handshake(se).await?;
                Ok((
                    Or::lhs(client),
                    Or::lhs(async {
                        conn.await;
                    }),
                ))
            }
            Self::Rhs { r } => {
                let (client, conn) = r.handshake(se).await?;
                Ok((
                    Or::rhs(client),
                    Or::rhs(async {
                        conn.await;
                    }),
                ))
            }
        }
    }
}

impl<B, L, R> Client<B> for Or<L, R>
where
    L: Client<B>,
    R: Client<B>,
{
    type Body = Or<L::Body, R::Body>;

    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error> {
        match self {
            Self::Lhs { l } => Ok(l.send(req).await?.map(Or::lhs)),
            Self::Rhs { r } => Ok(r.send(req).await?.map(Or::rhs)),
        }
    }
}

impl<L, R> Future for Or<L, R>
where
    L: Future,
    R: Future<Output = L::Output>,
{
    type Output = L::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        match self.project() {
            PinnedOr::Lhs { l } => l.poll(cx),
            PinnedOr::Rhs { r } => r.poll(cx),
        }
    }
}

impl<L, R> Stream for Or<L, R>
where
    L: Stream,
    R: Stream<Item = L::Item>,
{
    type Item = L::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.project() {
            PinnedOr::Lhs { l } => l.poll_next(cx),
            PinnedOr::Rhs { r } => r.poll_next(cx),
        }
    }
}
