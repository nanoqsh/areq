use {
    areq::{
        body::{Body, IntoBody},
        bytes::Buf,
    },
    serde::{Deserialize, de::DeserializeOwned},
    std::io::Error,
};

/// Extension methods for a [body](Body) for working with JSON.
pub trait JsonBodyExt: IntoBody {
    async fn json<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let mut body = self.into_body();
        let cap = body.size_hint().size().unwrap_or(1024);
        let mut v = Vec::with_capacity(cap as usize);
        while let Some(res) = body.chunk().await {
            v.extend_from_slice(res?.chunk());
        }

        let res = serde_json::from_slice(&v)?;
        Ok(res)
    }

    async fn json_from_buffer<'de, T>(self, buffer: &'de mut Vec<u8>) -> Result<T, Error>
    where
        T: Deserialize<'de>,
    {
        let mut body = self.into_body();

        let cap = body.size_hint().size().unwrap_or(1024);
        buffer.clear();
        buffer.reserve(cap as usize);
        while let Some(res) = body.chunk().await {
            buffer.extend_from_slice(res?.chunk());
        }

        let res = serde_json::from_slice(buffer)?;
        Ok(res)
    }
}

impl<B> JsonBodyExt for B where B: IntoBody {}
