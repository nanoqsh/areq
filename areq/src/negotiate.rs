use crate::alt::Alt;

pub trait Negotiate {
    type Handshake;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake>;
    fn support(&self) -> impl Iterator<Item = &'static [u8]>;
}

pub struct Select<L, R>(pub L, pub R);

impl<L, R> Negotiate for Select<L, R>
where
    L: Negotiate,
    R: Negotiate,
{
    type Handshake = Alt<L::Handshake, R::Handshake>;

    fn negotiate(self, proto: &[u8]) -> Option<Self::Handshake> {
        let Self(l, r) = self;

        l.negotiate(proto)
            .map(Alt::lhs)
            .or_else(|| r.negotiate(proto).map(Alt::rhs))
    }

    fn support(&self) -> impl Iterator<Item = &'static [u8]> {
        let Self(l, r) = self;

        l.support().chain(r.support())
    }
}
