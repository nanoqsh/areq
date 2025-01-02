use {
    bytes::Bytes,
    futures_lite::{future, prelude::*, stream},
    http_body_util::StreamBody,
    hyper::body::Frame,
    smol::{channel::Receiver, Timer},
    std::{convert::Infallible, pin::Pin, time::Duration},
};

pub type EventStream =
    StreamBody<Pin<Box<dyn Stream<Item = Result<Frame<Bytes>, Infallible>> + Send>>>;

pub fn event_stream(consume: Receiver<()>) -> EventStream {
    let stream = stream::unfold(consume, |consume| async {
        let recv = async {
            consume.recv().await.ok()?;
            const { Some(Bytes::from_static(b"data: {}\n\n")) }
        };

        let ping = async {
            Timer::after(Duration::from_secs(15)).await;
            const { Some(Bytes::from_static(b":\n\n")) }
        };

        let chunk = future::or(recv, ping).await?;
        Some((chunk, consume))
    })
    .map(Frame::data)
    .map(Ok)
    .boxed();

    StreamBody::new(stream)
}
