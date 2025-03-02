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

impl<I, L, R> Handshake<I> for Alt<L, R>
where
    L: Handshake<I>,
    R: Handshake<I>,
{
    type Client<B>
        = Alt<L::Client<B>, R::Client<B>>
    where
        B: IntoBody;

    async fn handshake<B>(self, se: Session<I>) -> Result<(Self::Client<B>, impl Future), Error>
    where
        B: IntoBody,
    {
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

        Ok((client, async {
            conn.await;
        }))
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
}

impl<L, R> Future for Alt<L, R>
where
    L: Future,
    R: Future,
{
    type Output = Alt<L::Output, R::Output>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project() {
            Pinned::Lhs { l } => l.poll(cx).map(Alt::lhs),
            Pinned::Rhs { r } => r.poll(cx).map(Alt::rhs),
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
