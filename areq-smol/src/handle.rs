use {areq::Session, futures_lite::future, std::io::Error};

#[cfg(feature = "rtn")]
use areq::HandshakeWith;

pub trait Handle<C, U, F> {
    #[expect(async_fn_in_trait)]
    async fn handle(self, f: F) -> Result<U, Error>;
}

impl<C, U, F, N> Handle<C, U, F> for (C, N)
where
    F: AsyncFnOnce(C) -> Result<U, Error>,
    N: Future,
{
    async fn handle(self, f: F) -> Result<U, Error> {
        let (client, conn) = self;

        let io = async {
            conn.await;
            Ok(())
        };

        Box::pin(future::try_zip(io, f(client))) // box large futures
            .await
            .map(|(_, res)| res)
    }
}

/// Asserts the handle is `Send` if the client and task of `Handshake` are also `Send`.
#[cfg(feature = "rtn")]
fn _handle_is_send<H, I, B>(h: H, se: Session<I>)
where
    H: HandshakeWith<I, B, Client: Send, Task: Send>,
{
    fn is_send<S>(s: S) -> S
    where
        S: Send,
    {
        s
    }

    _ = async {
        let p = h.handshake(se).await.expect("comptime assertion");

        // Also the callback must be `Send`
        let callback = is_send(async |_| Ok(()));

        _ = is_send(p.handle(callback));
    };
}
