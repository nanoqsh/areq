use {
    crate::{
        client::Client,
        negotiate::Negotiate,
        proto::{Error, Handshake, Request, Response, Session},
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
    #[project = Pinned]
    pub enum Alt<L, R> {
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

impl<L, R> Alt<L, R> {
    pub fn lhs(l: L) -> Self {
        Self::Lhs { l }
    }

    pub fn rhs(r: R) -> Self {
        Self::Rhs { r }
    }
}

impl<I, B, L, R> Handshake<I, B> for Alt<L, R>
where
    L: Handshake<I, B>,
    R: Handshake<I, B>,
{
    type Client = Alt<L::Client, R::Client>;

    async fn handshake(
        self,
        se: Session<I>,
    ) -> Result<(Self::Client, impl Future<Output = ()>), Error> {
        let (client, conn) = match self {
            Self::Lhs { l } => {
                let (client, conn) = l.handshake(se).await?;
                (Alt::lhs(client), Alt::lhs(conn))
            }
            Self::Rhs { r } => {
                let (client, conn) = r.handshake(se).await?;
                (Alt::rhs(client), Alt::rhs(conn))
            }
        };

        Ok((client, conn))
    }
}

impl<L, R> Negotiate for Alt<L, R>
where
    L: Negotiate,
    R: Negotiate,
{
    type Handshake = Alt<L::Handshake, R::Handshake>;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake> {
        match self {
            Self::Lhs { l } => l.negotiate(proto).map(Alt::lhs),
            Self::Rhs { r } => r.negotiate(proto).map(Alt::rhs),
        }
    }

    fn support(&self) -> impl Iterator<Item = &'static [u8]> {
        match self {
            Self::Lhs { l } => Alt::lhs(l.support()),
            Self::Rhs { r } => Alt::rhs(r.support()),
        }
    }
}

impl<B, L, R> Client<B> for Alt<L, R>
where
    L: Client<B>,
    R: Client<B>,
{
    type Body = Alt<L::Body, R::Body>;

    async fn send(&mut self, req: Request<B>) -> Result<Response<Self::Body>, Error> {
        match self {
            Self::Lhs { l } => Ok(l.send(req).await?.map(Alt::lhs)),
            Self::Rhs { r } => Ok(r.send(req).await?.map(Alt::rhs)),
        }
    }

    fn try_clone(&self) -> Option<Self> {
        match self {
            Self::Lhs { l } => l.try_clone().map(Self::lhs),
            Self::Rhs { r } => r.try_clone().map(Self::rhs),
        }
    }
}

impl<L, R> Future for Alt<L, R>
where
    L: Future,
    R: Future<Output = L::Output>,
{
    type Output = L::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            Pinned::Lhs { l } => l.poll(cx),
            Pinned::Rhs { r } => r.poll(cx),
        }
    }
}

impl<L, R> Body for Alt<L, R>
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

impl<L, R> Iterator for Alt<L, R>
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
