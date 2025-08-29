use {
    areq::{HandshakeWith, Session},
    futures_lite::future,
    std::io::Error,
};

pub trait Handle<C, U, F>
where
    // Add more context for better DX with rust analyzer
    F: AsyncFnOnce(&mut C) -> Result<U, Error>,
{
    async fn handle(self, f: F) -> Result<U, Error>;
}

impl<C, U, F, N> Handle<C, U, F> for (C, N)
where
    F: AsyncFnOnce(&mut C) -> Result<U, Error>,
    N: Future,
{
    #[inline]
    async fn handle(self, f: F) -> Result<U, Error> {
        let (mut client, conn) = self;

        let io = async {
            conn.await;
            Ok(())
        };

        let (_, res) = Box::pin(future::try_zip(io, f(&mut client))) // box large futures
            .await?;

        Ok(res)
    }
}

/// Asserts the handle is `Send` if the client and task of `Handshake` are also `Send`.
fn _handle_is_send<H, I, B>(h: H, se: Session<I>)
where
    H: HandshakeWith<I, B, Client: Send, Task: Send>,
{
    fn assert_send<S>(s: S) -> S
    where
        S: Send,
    {
        s
    }

    _ = async {
        let p = h.handshake(se).await.expect("comptime assertion");

        // Also the callback must be `Send`
        let callback = assert_send(async |_: &mut _| Ok(()));

        _ = assert_send(p.handle(callback));
    };
}
