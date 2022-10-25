use std::marker::PhantomData;

use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::status::{Code, Status};

const HEADER_SIZE: usize = 5;

pub(crate) trait Encoder {
    type Error;

    fn encode<T: prost::Message>(&self, message: &T) -> Result<Bytes, Self::Error>;
}

pub(crate) struct ProstEncoder {}

impl ProstEncoder {
    pub fn new() -> Self {
        ProstEncoder {}
    }
}

impl Encoder for ProstEncoder {
    type Error = Status;

    fn encode<T: prost::Message>(&self, message: &T) -> Result<Bytes, Self::Error> {
        let mut buf = BytesMut::new();

        // reserver header
        buf.put_bytes(0, 5);
        T::encode(message, &mut buf)?;

        let len = buf.len() - 5;

        let mut header = &mut buf[..HEADER_SIZE];
        // no compress
        header.put_u8(0x00);

        // put data len
        header.put_u32(len as u32);

        Ok(buf.freeze())
    }
}

pub(crate) trait Decoder {
    type Item: prost::Message + Default;
    type Error;

    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;
}

pub(crate) struct ProstDecoder<T> {
    message: PhantomData<T>,
}

impl<T> ProstDecoder<T> {
    pub fn new() -> Self {
        ProstDecoder {
            message: PhantomData::default(),
        }
    }
}

impl<T: prost::Message + Default> Decoder for ProstDecoder<T> {
    type Item = T;
    type Error = Status;

    fn decode(&self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.len() < HEADER_SIZE {
            return Ok(None);
        }

        if buf[0] != 0 {
            return Err(Status::new(
                Code::Unimplemented,
                "do not support compression",
            ));
        }

        let i = &buf[1..5];

        let len = u32::from_be_bytes(i.try_into().unwrap());

        if buf.len() < HEADER_SIZE + len as usize {
            return Ok(None);
        }

        buf.advance(HEADER_SIZE);

        let buf = buf.split_to(len as usize);

        let message = Self::Item::decode(buf)?;

        Ok(Some(message))
    }
}
