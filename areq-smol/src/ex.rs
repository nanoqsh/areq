use {
    areq::{Error, HandshakeWith, Session},
    async_executor::{Executor, LocalExecutor, Task},
    async_net::TcpStream,
};

pub async fn spawn<'ex, H, B>(
    ex: &Executor<'ex>,
    handshake: H,
    se: Session<TcpStream>,
) -> Result<Spawned<H::Client>, Error>
where
    H: HandshakeWith<TcpStream, B, Task: Send + 'ex>,
{
    let (client, conn) = handshake.handshake(se).await?;
    let task = ex.spawn(async {
        conn.await;
    });

    Ok(Spawned { client, task })
}

pub async fn spawn_local<'ex, H, B>(
    ex: &LocalExecutor<'ex>,
    handshake: H,
    se: Session<TcpStream>,
) -> Result<Spawned<H::Client>, Error>
where
    H: HandshakeWith<TcpStream, B, Task: 'ex>,
{
    let (client, conn) = handshake.handshake(se).await?;
    let task = ex.spawn(async {
        conn.await;
    });

    Ok(Spawned { client, task })
}

pub struct Spawned<C> {
    pub client: C,
    pub task: Task<()>,
}

impl<C> Spawned<C> {
    pub fn detach(self) -> C {
        self.task.detach();
        self.client
    }
}
