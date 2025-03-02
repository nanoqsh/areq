use {
    crate::{
        negotiate::Negotiate,
        proto::{Client, Error, Handshake, Request, Response, Session},
    },
    areq_body::prelude::*,
    futures_lite::prelude::*,
    std::{
        io,
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

impl<I, L, R> Handshake<I> for Or<L, R>
where
    L: Handshake<I>,
    R: Handshake<I>,
{
    type Client<B>
        = Or<L::Client<B>, R::Client<B>>
    where
        B: IntoBody;

    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
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

impl<L, R> Negotiate for Or<L, R>
where
    L: Negotiate,
    R: Negotiate,
{
    type Handshake = Or<L::Handshake, R::Handshake>;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake> {
        match self {
            Self::Lhs { l } => l.negotiate(proto).map(Or::lhs),
            Self::Rhs { r } => r.negotiate(proto).map(Or::rhs),
        }
    }

    fn support(&self) -> impl Iterator<Item = &'static [u8]> {
        match self {
            Self::Lhs { l } => Or::lhs(l.support()),
            Self::Rhs { r } => Or::rhs(r.support()),
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

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            PinnedOr::Lhs { l } => l.poll(cx),
            PinnedOr::Rhs { r } => r.poll(cx),
        }
    }
}

impl<L, R> Body for Or<L, R>
where
    L: Body,
    R: Body<Chunk = L::Chunk>,
{
    type Chunk = L::Chunk;

    async fn chunk(&mut self) -> Option<Result<Self::Chunk, io::Error>> {
        match self {
            Self::Lhs { l } => l.chunk().await,
            Self::Rhs { r } => r.chunk().await,
        }
    }

    fn size_hint(&self) -> Hint {
        match self {
            Self::Lhs { l } => l.size_hint(),
            Self::Rhs { r } => r.size_hint(),
        }
    }
}

impl<L, R> Iterator for Or<L, R>
where
    L: Iterator,
    R: Iterator<Item = L::Item>,
{
    type Item = L::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Lhs { l } => l.next(),
            Self::Rhs { r } => r.next(),
        }
    }
}
