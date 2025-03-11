use crate::proto::{Error, Handshake};

pub trait HandshakeWith<I, B>:
    Handshake<I, B, handshake(..): Future<Output = Result<(Self::Client, Self::Task), Error>>>
{
    type Task: Future<Output = ()>;
}

impl<H, I, B, T> HandshakeWith<I, B> for H
where
    H: Handshake<I, B, handshake(..): Future<Output = Result<(Self::Client, T), Error>>>,
    T: Future<Output = ()>,
{
    type Task = T;
}
